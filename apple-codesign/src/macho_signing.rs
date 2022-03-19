// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Signing binaries.
//!
//! This module contains code for signing binaries.

use {
    crate::{
        code_directory::{CodeDirectoryBlob, CodeSignatureFlags},
        code_hash::compute_code_hashes,
        code_requirement::{CodeRequirementExpression, CodeRequirements},
        entitlements::plist_to_executable_segment_flags,
        error::AppleCodesignError,
        macho::{
            create_superblob, find_macho_targeting, AppleSignable, Blob, BlobWrapperBlob,
            CodeSigningMagic, CodeSigningSlot, Digest, DigestType, EmbeddedSignature,
            EntitlementsBlob, EntitlementsDerBlob, RequirementSetBlob, RequirementType,
        },
        policy::derive_designated_requirements,
        signing::{DesignatedRequirementMode, SettingsScope, SigningSettings},
        ExecutableSegmentFlags,
    },
    bcder::{encode::PrimitiveContent, Oid},
    bytes::Bytes,
    cryptographic_message_syntax::{asn1::rfc5652::OID_ID_DATA, SignedDataBuilder, SignerBuilder},
    goblin::mach::{
        constants::{SEG_LINKEDIT, SEG_PAGEZERO},
        load_command::{
            CommandVariant, LinkeditDataCommand, SegmentCommand32, SegmentCommand64,
            LC_CODE_SIGNATURE, SIZEOF_LINKEDIT_DATA_COMMAND,
        },
        parse_magic_and_ctx, Mach, MachO,
    },
    log::{info, warn},
    scroll::{ctx::SizeWith, IOwrite},
    std::{borrow::Cow, cmp::Ordering, collections::HashMap, io::Write},
    tugger_apple::create_universal_macho,
    x509_certificate::{rfc5652::AttributeValue, DigestAlgorithm},
};

/// OID for signed attribute containing plist of code directory hashes.
///
/// 1.2.840.113635.100.9.1.
const CDHASH_PLIST_OID: bcder::ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 9, 1]);

/// OID for signed attribute containing the SHA-256 of code directory digests.
///
/// 1.2.840.113635.100.9.2
const CDHASH_SHA256_OID: bcder::ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 9, 2]);

/// Obtain the XML plist containing code directory hashes.
///
/// This plist is embedded as a signed attribute in the CMS signature.
pub fn create_code_directory_hashes_plist<'a>(
    code_directories: impl Iterator<Item = &'a CodeDirectoryBlob<'a>>,
    digest_type: DigestType,
) -> Result<Vec<u8>, AppleCodesignError> {
    let hashes = code_directories
        .map(|cd| {
            let mut digest = cd.digest_with(digest_type)?;

            // While we may use stronger digests, it appears that the XML in the
            // signed attribute is always truncated so it is the length of a SHA-1
            // digest.
            digest.truncate(20);

            Ok(plist::Value::Data(digest))
        })
        .collect::<Result<Vec<_>, AppleCodesignError>>()?;

    let mut plist = plist::Dictionary::new();
    plist.insert("cdhashes".to_string(), plist::Value::Array(hashes));

    let mut buffer = Vec::<u8>::new();
    plist::Value::from(plist)
        .to_writer_xml(&mut buffer)
        .map_err(AppleCodesignError::CodeDirectoryPlist)?;
    // We also need to include a trailing newline to conform with Apple's XML
    // writer.
    buffer.push(b'\n');

    Ok(buffer)
}

/// Derive a new Mach-O binary with new signature data.
fn create_macho_with_signature(
    macho_data: &[u8],
    macho: &MachO,
    signature_data: &[u8],
) -> Result<Vec<u8>, AppleCodesignError> {
    // This should have already been called. But we do it again out of paranoia.
    macho.check_signing_capability()?;

    // The assumption made by checking_signing_capability() is that signature data
    // is at the end of the __LINKEDIT segment. So the replacement segment is the
    // existing segment truncated at the signature start followed by the new signature
    // data.
    let new_linkedit_segment_size = macho
        .linkedit_data_before_signature()
        .ok_or(AppleCodesignError::MissingLinkedit)?
        .len()
        + signature_data.len();

    // `codesign` rounds up the segment's vmsize to the nearest 16kb boundary.
    // We emulate that behavior.
    let remainder = new_linkedit_segment_size % 16384;
    let new_linkedit_segment_vmsize = if remainder == 0 {
        new_linkedit_segment_size
    } else {
        new_linkedit_segment_size + 16384 - remainder
    };

    assert!(new_linkedit_segment_vmsize >= new_linkedit_segment_size);
    assert_eq!(new_linkedit_segment_vmsize % 16384, 0);

    let mut cursor = std::io::Cursor::new(Vec::<u8>::new());

    // Mach-O data structures are variable endian. So use the endian defined
    // by the magic when writing.
    let ctx = parse_magic_and_ctx(macho_data, 0)?
        .1
        .expect("context should have been parsed before");

    // If there isn't a code signature presently, we'll need to introduce a load
    // command for it.
    let mut header = macho.header;
    if macho.code_signature_load_command().is_none() {
        header.ncmds += 1;
        header.sizeofcmds += SIZEOF_LINKEDIT_DATA_COMMAND as u32;
    }

    cursor.iowrite_with(header, ctx)?;

    // Following the header are load commands. We need to update load commands
    // to reflect changes to the signature size and __LINKEDIT segment size.

    let mut seen_signature_load_command = false;

    for load_command in &macho.load_commands {
        let original_command_data =
            &macho_data[load_command.offset..load_command.offset + load_command.command.cmdsize()];

        let written_len = match &load_command.command {
            CommandVariant::CodeSignature(command) => {
                seen_signature_load_command = true;

                let mut command = *command;
                command.datasize = signature_data.len() as _;

                cursor.iowrite_with(command, ctx.le)?;

                LinkeditDataCommand::size_with(&ctx.le)
            }
            CommandVariant::Segment32(segment) => {
                let segment = match segment.name() {
                    Ok(SEG_LINKEDIT) => {
                        let mut segment = *segment;
                        segment.filesize = new_linkedit_segment_size as _;
                        segment.vmsize = new_linkedit_segment_vmsize as _;

                        segment
                    }
                    _ => *segment,
                };

                cursor.iowrite_with(segment, ctx.le)?;

                SegmentCommand32::size_with(&ctx.le)
            }
            CommandVariant::Segment64(segment) => {
                let segment = match segment.name() {
                    Ok(SEG_LINKEDIT) => {
                        let mut segment = *segment;
                        segment.filesize = new_linkedit_segment_size as _;
                        segment.vmsize = new_linkedit_segment_vmsize as _;

                        segment
                    }
                    _ => *segment,
                };

                cursor.iowrite_with(segment, ctx.le)?;

                SegmentCommand64::size_with(&ctx.le)
            }
            _ => {
                // Reflect the original bytes.
                cursor.write_all(original_command_data)?;
                original_command_data.len()
            }
        };

        // For the commands we mutated ourselves, there may be more data after the
        // load command header. Write it out if present.
        cursor.write_all(&original_command_data[written_len..])?;
    }

    // If we didn't see a signature load command, write one out now.
    if !seen_signature_load_command {
        let command = LinkeditDataCommand {
            cmd: LC_CODE_SIGNATURE,
            cmdsize: SIZEOF_LINKEDIT_DATA_COMMAND as _,
            dataoff: macho.code_limit_binary_offset()? as _,
            datasize: signature_data.len() as _,
        };

        cursor.iowrite_with(command, ctx.le)?;
    }

    // Write out segments, updating the __LINKEDIT segment when we encounter it.
    for segment in macho.segments.iter() {
        assert!(segment.fileoff == 0 || segment.fileoff == cursor.position());

        // The initial __PAGEZERO segment contains no data (it is the magic and load
        // commands) and overlaps with the __TEXT segment, which has .fileoff =0, so
        // we ignore it.
        if matches!(segment.name(), Ok(SEG_PAGEZERO)) {
            continue;
        }

        match segment.name() {
            Ok(SEG_LINKEDIT) => {
                cursor.write_all(
                    macho
                        .linkedit_data_before_signature()
                        .expect("__LINKEDIT segment data should resolve"),
                )?;
                cursor.write_all(signature_data)?;
            }
            _ => {
                // At least the __TEXT segment has .fileoff = 0, which has it
                // overlapping with already written data. So only write segment
                // data new to the writer.
                if segment.fileoff < cursor.position() {
                    let remaining =
                        &segment.data[cursor.position() as usize..segment.filesize as usize];
                    cursor.write_all(remaining)?;
                } else {
                    cursor.write_all(segment.data)?;
                }
            }
        }
    }

    Ok(cursor.into_inner())
}

/// Mach-O binary signer.
///
/// This type provides a high-level interface for signing Mach-O binaries.
/// It handles parsing and rewriting Mach-O binaries and contains most of the
/// functionality for producing signatures for individual Mach-O binaries.
///
/// Signing of both single architecture and fat/universal binaries is supported.
///
/// # Circular Dependency
///
/// There is a circular dependency between the generation of the Code Directory
/// present in the embedded signature and the Mach-O binary. See the note
/// in [crate::specification] for the gory details. The tl;dr is the Mach-O
/// data up to the signature data needs to be digested. But that digested data
/// contains load commands that reference the signature data and its size, which
/// can't be known until the Code Directory, CMS blob, and SuperBlob are all
/// created.
///
/// Our solution to this problem is to estimate the size of the embedded
/// signature data and then pad the unused data will 0s.
#[derive(Debug)]
pub struct MachOSigner<'data> {
    /// Raw data backing parsed Mach-O binary.
    macho_data: &'data [u8],

    /// Parsed Mach-O binaries.
    machos: Vec<MachO<'data>>,
}

impl<'data> MachOSigner<'data> {
    /// Construct a new instance from unparsed data representing a Mach-O binary.
    ///
    /// The data will be parsed as a Mach-O binary (either single arch or fat/universal)
    /// and validated that we are capable of signing it.
    pub fn new(macho_data: &'data [u8]) -> Result<Self, AppleCodesignError> {
        let mach = Mach::parse(macho_data)?;

        let machos = match mach {
            Mach::Binary(macho) => {
                macho.check_signing_capability()?;

                vec![macho]
            }
            Mach::Fat(multiarch) => {
                let mut machos = vec![];

                for index in 0..multiarch.narches {
                    let macho = multiarch.get(index)?;
                    macho.check_signing_capability()?;

                    machos.push(macho);
                }

                machos
            }
        };

        Ok(Self { macho_data, machos })
    }

    /// Write signed Mach-O data to the given writer using signing settings.
    pub fn write_signed_binary(
        &self,
        settings: &SigningSettings,
        writer: &mut impl Write,
    ) -> Result<(), AppleCodesignError> {
        // Implementing a true streaming writer requires calculating final sizes
        // of all binaries so fat header offsets and sizes can be written first. We take
        // the easy road and buffer individual Mach-O binaries internally.

        let binaries = self
            .machos
            .iter()
            .enumerate()
            .map(|(index, original_macho)| {
                info!("signing Mach-O binary at index {}", index);
                let settings =
                    settings.as_nested_macho_settings(index, original_macho.header.cputype());

                let signature_len = original_macho.estimate_embedded_signature_size(&settings)?;

                // Derive an intermediate Mach-O with placeholder NULLs for signature
                // data so Code Directory digests over the load commands are correct.
                let placeholder_signature_data = b"\0".repeat(signature_len);

                let intermediate_macho_data = create_macho_with_signature(
                    self.macho_data(index),
                    original_macho,
                    &placeholder_signature_data,
                )?;

                // A nice side-effect of this is that it catches bugs if we write malformed Mach-O!
                let intermediate_macho = MachO::parse(&intermediate_macho_data, 0)?;

                let mut signature_data = self.create_superblob(
                    &settings,
                    self.macho_data(index),
                    &intermediate_macho,
                    original_macho.code_signature()?.as_ref(),
                )?;
                info!("total signature size: {} bytes", signature_data.len());

                // The Mach-O writer adjusts load commands based on the signature length. So pad
                // with NULLs to get to our placeholder length.
                match signature_data.len().cmp(&placeholder_signature_data.len()) {
                    Ordering::Greater => {
                        return Err(AppleCodesignError::SignatureDataTooLarge);
                    }
                    Ordering::Equal => {}
                    Ordering::Less => {
                        signature_data.extend_from_slice(
                            &b"\0".repeat(placeholder_signature_data.len() - signature_data.len()),
                        );
                    }
                }

                create_macho_with_signature(
                    &intermediate_macho_data,
                    &intermediate_macho,
                    &signature_data,
                )
            })
            .collect::<Result<Vec<_>, AppleCodesignError>>()?;

        if binaries.len() > 1 {
            create_universal_macho(writer, binaries.iter().map(|x| x.as_slice()))?;
        } else {
            writer.write_all(&binaries[0])?;
        }

        Ok(())
    }

    /// Derive the data slice belonging to a Mach-O binary.
    fn macho_data(&self, index: usize) -> &[u8] {
        match Mach::parse(self.macho_data).expect("should reparse without error") {
            Mach::Binary(_) => self.macho_data,
            Mach::Fat(multiarch) => {
                let arch = multiarch
                    .iter_arches()
                    .nth(index)
                    .expect("bad index")
                    .expect("reparse should have worked");

                let end_offset = arch.offset as usize + arch.size as usize;

                &self.macho_data[arch.offset as usize..end_offset]
            }
        }
    }

    /// Create data constituting the SuperBlob to be embedded in the `__LINKEDIT` segment.
    ///
    /// The superblob contains the code directory, any extra blobs, and an optional
    /// CMS structure containing a cryptographic signature.
    ///
    /// This takes an explicit Mach-O to operate on due to a circular dependency
    /// between writing out the Mach-O and digesting its content. See the note
    /// in [MachOSigner] for details.
    pub fn create_superblob(
        &self,
        settings: &SigningSettings,
        macho_data: &[u8],
        macho: &MachO,
        previous_signature: Option<&EmbeddedSignature>,
    ) -> Result<Vec<u8>, AppleCodesignError> {
        let code_directory =
            self.create_code_directory(settings, macho_data, macho, previous_signature)?;
        info!("code directory version: {}", code_directory.version);

        // By convention, the Code Directory goes first.
        let mut blobs = vec![(
            CodeSigningSlot::CodeDirectory,
            code_directory.to_blob_bytes()?,
        )];
        blobs.extend(self.create_special_blobs(settings, previous_signature)?);

        // And the CMS signature goes last.
        if settings.signing_key().is_some() {
            blobs.push((
                CodeSigningSlot::Signature,
                BlobWrapperBlob::from_data(&self.create_cms_signature(settings, &code_directory)?)
                    .to_blob_bytes()?,
            ));
        }

        create_superblob(CodeSigningMagic::EmbeddedSignature, blobs.iter())
    }

    /// Create a CMS `SignedData` structure containing a cryptographic signature.
    ///
    /// This becomes the content of the `EmbeddedSignature` blob in the `Signature` slot.
    ///
    /// This function will error if a signing key has not been specified.
    ///
    /// This takes an explicit Mach-O to operate on due to a circular dependency
    /// between writing out the Mach-O and digesting its content. See the note
    /// in [MachOSigner] for details.
    pub fn create_cms_signature(
        &self,
        settings: &SigningSettings,
        code_directory: &CodeDirectoryBlob,
    ) -> Result<Vec<u8>, AppleCodesignError> {
        let (signing_key, signing_cert) = settings
            .signing_key()
            .ok_or(AppleCodesignError::NoSigningCertificate)?;

        if let Some(cn) = signing_cert.subject_common_name() {
            warn!("creating cryptographic signature with certificate {}", cn);
        }

        // We need the blob serialized content of the code directory to compute
        // the message digest using alternate data.
        let code_directory_raw = code_directory.to_blob_bytes()?;

        // We need an XML plist containing code directory hashes to include as a signed
        // attribute.
        let code_directories = vec![code_directory];
        let code_directory_hashes_plist = create_code_directory_hashes_plist(
            code_directories.into_iter(),
            code_directory.hash_type,
        )?;

        let signer = SignerBuilder::new(signing_key, signing_cert.clone())
            .message_id_content(code_directory_raw)
            .signed_attribute_octet_string(
                Oid(Bytes::copy_from_slice(CDHASH_PLIST_OID.as_ref())),
                &code_directory_hashes_plist,
            );

        // If we're using a digest beyond SHA-1, that digest is included as an additional
        // signed attribute. However, Apple is using unregistered OIDs here. We only know about
        // the SHA-256 one. It exists as an `(OID, OCTET STRING)` value where the OID
        // is 2.16.840.1.101.3.4.2.1, which is registered.
        let signer = if code_directory.hash_type == DigestType::Sha256 {
            let digest = code_directory.digest_with(DigestType::Sha256)?;

            signer.signed_attribute(
                Oid(CDHASH_SHA256_OID.as_ref().into()),
                vec![AttributeValue::new(bcder::Captured::from_values(
                    bcder::Mode::Der,
                    bcder::encode::sequence((
                        Oid::from(DigestAlgorithm::Sha256).encode_ref(),
                        bcder::OctetString::new(digest.into()).encode_ref(),
                    )),
                ))],
            )
        } else {
            signer
        };

        let signer = if let Some(time_stamp_url) = settings.time_stamp_url() {
            info!("Using time-stamp server {}", time_stamp_url);
            signer.time_stamp_url(time_stamp_url.clone())?
        } else {
            signer
        };

        let der = SignedDataBuilder::default()
            // The default is `signed-data`. But Apple appears to use the `data` content-type,
            // in violation of RFC 5652 Section 5, which says `signed-data` should be
            // used when there are signatures.
            .content_type(Oid(OID_ID_DATA.as_ref().into()))
            .signer(signer)
            .certificates(settings.certificate_chain().iter().cloned())
            .build_der()?;

        Ok(der)
    }

    /// Attempt to resolve the binary identifier to use.
    ///
    /// If signing settings have defined one, use it. Otherwise use the last
    /// identifier on the binary, if present. Otherwise error.
    fn get_binary_identifier(
        &self,
        settings: &SigningSettings,
        previous_signature: Option<&EmbeddedSignature>,
    ) -> Result<String, AppleCodesignError> {
        let previous_cd =
            previous_signature.and_then(|signature| signature.code_directory().unwrap_or(None));

        match settings.binary_identifier(SettingsScope::Main) {
            Some(ident) => Ok(ident.to_string()),
            None => {
                if let Some(previous_cd) = &previous_cd {
                    Ok(previous_cd.ident.to_string())
                } else {
                    Err(AppleCodesignError::NoIdentifier)
                }
            }
        }
    }

    /// Create the `CodeDirectory` for the current configuration.
    ///
    /// This takes an explicit Mach-O to operate on due to a circular dependency
    /// between writing out the Mach-O and digesting its content. See the note
    /// in [MachOSigner] for details.
    pub fn create_code_directory(
        &self,
        settings: &SigningSettings,
        macho_data: &[u8],
        macho: &MachO,
        previous_signature: Option<&EmbeddedSignature>,
    ) -> Result<CodeDirectoryBlob<'static>, AppleCodesignError> {
        // TODO support defining or filling in proper values for fields with
        // static values.

        let target = find_macho_targeting(macho_data, macho)?;

        if let Some(target) = &target {
            info!(
                "binary targets {} >= {} with SDK {}",
                target.platform, target.minimum_os_version, target.sdk_version,
            );
        }

        let previous_cd =
            previous_signature.and_then(|signature| signature.code_directory().unwrap_or(None));

        let mut flags = CodeSignatureFlags::empty();

        match settings.code_signature_flags(SettingsScope::Main) {
            Some(additional) => flags |= additional,
            None => {
                if let Some(previous_cd) = &previous_cd {
                    flags |= previous_cd.flags;
                }
            }
        }

        // The adhoc flag is set when there is no CMS signature.
        if settings.signing_key().is_none() {
            info!("creating ad-hoc signature");
            flags |= CodeSignatureFlags::ADHOC;
        } else {
            flags -= CodeSignatureFlags::ADHOC;
        }

        // Remove linker signed flag because we're not a linker.
        flags -= CodeSignatureFlags::LINKER_SIGNED;

        // Code limit fields hold the file offset at which code digests stop. This
        // is the file offset in the `__LINKEDIT` segment when the embedded signature
        // SuperBlob begins.
        let (code_limit, code_limit_64) = match macho.code_limit_binary_offset()? {
            x if x > u32::MAX as u64 => (0, Some(x)),
            x => (x as u32, None),
        };

        let platform = 0;
        let page_size = 4096u32;

        let (exec_seg_base, exec_seg_limit) = macho.executable_segment_boundary()?;
        let (exec_seg_base, exec_seg_limit) = (Some(exec_seg_base), Some(exec_seg_limit));

        let mut exec_seg_flags = None;

        match settings.executable_segment_flags(SettingsScope::Main) {
            Some(flags) => {
                info!(
                    "using executable segment flags from signing settings ({:?})",
                    flags
                );
                exec_seg_flags = Some(flags);
            }
            None => {
                if let Some(previous_cd) = &previous_cd {
                    if let Some(flags) = previous_cd.exec_seg_flags {
                        info!(
                            "using executable segment flags from previous code directory ({:?})",
                            flags
                        );
                        exec_seg_flags = Some(flags);
                    }
                }
            }
        }

        // Entitlements can influence the executable segment flags. So make sure
        // flags derived from entitlements are always set.
        if let Some(entitlements) = settings.entitlements_plist(SettingsScope::Main) {
            let implied_flags = plist_to_executable_segment_flags(entitlements);

            if !implied_flags.is_empty() {
                info!(
                    "entitlements imply executable segment flags: {:?}",
                    implied_flags
                );

                exec_seg_flags = Some(
                    exec_seg_flags.unwrap_or_else(ExecutableSegmentFlags::empty) | implied_flags,
                );
            }
        }

        let runtime = match &previous_cd {
            Some(previous_cd) => previous_cd.runtime,
            None => None,
        };

        let code_hashes =
            compute_code_hashes(macho, *settings.digest_type(), Some(page_size as usize))?
                .into_iter()
                .map(|v| Digest { data: v.into() })
                .collect::<Vec<_>>();

        let mut special_hashes = self
            .create_special_blobs(settings, previous_signature)?
            .into_iter()
            .map(|(slot, data)| {
                Ok((
                    slot,
                    Digest {
                        data: settings.digest_type().digest(&data)?.into(),
                    },
                ))
            })
            .collect::<Result<HashMap<_, _>, AppleCodesignError>>()?;

        // There is no corresponding blob for the info plist data since it is provided
        // externally to the embedded signature.
        match settings.info_plist_data(SettingsScope::Main) {
            Some(data) => {
                special_hashes.insert(
                    CodeSigningSlot::Info,
                    Digest {
                        data: settings.digest_type().digest(data)?.into(),
                    },
                );
            }
            None => {
                if let Some(previous_cd) = &previous_cd {
                    if let Some(digest) = previous_cd.special_hashes.get(&CodeSigningSlot::Info) {
                        if !digest.is_null() {
                            special_hashes.insert(CodeSigningSlot::Info, digest.to_owned());
                        }
                    }
                }
            }
        }

        // There is no corresponding blob for resources data since it is provided
        // externally to the embedded signature.
        match settings.code_resources_data(SettingsScope::Main) {
            Some(data) => {
                special_hashes.insert(
                    CodeSigningSlot::ResourceDir,
                    Digest {
                        data: settings.digest_type().digest(data)?.into(),
                    }
                    .to_owned(),
                );
            }
            None => {
                if let Some(previous_cd) = &previous_cd {
                    if let Some(digest) = previous_cd
                        .special_hashes
                        .get(&CodeSigningSlot::ResourceDir)
                    {
                        if !digest.is_null() {
                            special_hashes.insert(CodeSigningSlot::ResourceDir, digest.to_owned());
                        }
                    }
                }
            }
        }

        let ident = Cow::Owned(self.get_binary_identifier(settings, previous_signature)?);

        let team_name = match settings.team_id() {
            Some(team_name) => Some(Cow::Owned(team_name.to_string())),
            None => {
                if let Some(previous_cd) = &previous_cd {
                    previous_cd
                        .team_name
                        .as_ref()
                        .map(|name| Cow::Owned(name.clone().into_owned()))
                } else {
                    None
                }
            }
        };

        let mut cd = CodeDirectoryBlob {
            version: 0,
            flags,
            code_limit,
            hash_size: settings.digest_type().hash_len()? as u8,
            hash_type: *settings.digest_type(),
            platform,
            page_size,
            spare2: 0,
            scatter_offset: None,
            spare3: None,
            code_limit_64,
            exec_seg_base,
            exec_seg_limit,
            exec_seg_flags,
            runtime,
            pre_encrypt_offset: None,
            linkage_hash_type: None,
            linkage_truncated: None,
            spare4: None,
            linkage_offset: None,
            linkage_size: None,
            ident,
            team_name,
            code_hashes,
            special_hashes,
        };

        cd.adjust_version(target);
        cd.clear_newer_fields();

        Ok(cd)
    }

    /// Create blobs that need to be written given the current configuration.
    ///
    /// This emits all blobs except `CodeDirectory` and `Signature`, which are
    /// special since they are derived from the blobs emitted here.
    ///
    /// The goal of this function is to emit data to facilitate the creation of
    /// a `CodeDirectory`, which requires hashing blobs.
    pub fn create_special_blobs(
        &self,
        settings: &SigningSettings,
        previous_signature: Option<&EmbeddedSignature>,
    ) -> Result<Vec<(CodeSigningSlot, Vec<u8>)>, AppleCodesignError> {
        let mut res = Vec::new();

        let mut requirements = CodeRequirements::default();

        match settings.designated_requirement(SettingsScope::Main) {
            DesignatedRequirementMode::Auto => {
                // If we are using an Apple-issued cert, this should automatically
                // derive appropriate designated requirements.
                if let Some((_, cert)) = settings.signing_key() {
                    info!("attempting to derive code requirements from signing certificate");
                    let identifier =
                        Some(self.get_binary_identifier(settings, previous_signature)?);

                    if let Some(expr) = derive_designated_requirements(cert, identifier)? {
                        requirements.push(expr);
                    }
                }
            }
            DesignatedRequirementMode::Explicit(exprs) => {
                info!("using provided code requirements");
                for expr in exprs {
                    requirements.push(CodeRequirementExpression::from_bytes(expr)?.0);
                }
            }
        }

        if !requirements.is_empty() {
            info!("code requirements: {}", requirements);

            let mut blob = RequirementSetBlob::default();
            requirements.add_to_requirement_set(&mut blob, RequirementType::Designated)?;

            res.push((CodeSigningSlot::RequirementSet, blob.to_blob_bytes()?));
        }

        if let Some(entitlements) = settings.entitlements_xml(SettingsScope::Main)? {
            info!("adding entitlements XML");
            let blob = EntitlementsBlob::from_string(&entitlements);

            res.push((CodeSigningSlot::Entitlements, blob.to_blob_bytes()?));
        }

        // The DER encoded entitlements weren't always present in the signature. The feature
        // appears to have been introduced in macOS 10.14 and is the default behavior as of
        // macOS 12 "when signing for all platforms." Since `codesign` appears to always add
        // this blob when entitlements are present, we mimic the behavior. But there may be
        // scenarios where we want to omit this.
        if let Some(value) = settings.entitlements_plist(SettingsScope::Main) {
            info!("adding entitlements DER");
            let blob = EntitlementsDerBlob::from_plist(value)?;

            res.push((CodeSigningSlot::EntitlementsDer, blob.to_blob_bytes()?));
        }

        Ok(res)
    }
}
