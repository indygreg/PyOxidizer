// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Signing binaries.
//!
//! This module contains code for signing binaries.

use {
    crate::{
        code_directory::{CodeDirectoryBlob, CodeSignatureFlags, ExecutableSegmentFlags},
        code_hash::compute_code_hashes,
        code_requirement::CodeRequirements,
        error::AppleCodesignError,
        macho::{
            create_superblob, find_signature_data, Blob, BlobWrapperBlob, CodeSigningMagic,
            CodeSigningSlot, Digest, DigestType, EmbeddedSignature, EntitlementsBlob,
            RequirementBlob, RequirementSetBlob, RequirementType,
        },
    },
    bytes::Bytes,
    cryptographic_message_syntax::{Certificate, SignedDataBuilder, SignerBuilder, SigningKey},
    goblin::mach::{
        constants::{SEG_LINKEDIT, SEG_PAGEZERO},
        fat::FAT_MAGIC,
        fat::{SIZEOF_FAT_ARCH, SIZEOF_FAT_HEADER},
        load_command::{CommandVariant, LinkeditDataCommand, SegmentCommand32, SegmentCommand64},
        parse_magic_and_ctx, Mach, MachO,
    },
    reqwest::{IntoUrl, Url},
    scroll::{ctx::SizeWith, IOwrite, Pwrite},
    std::{cmp::Ordering, collections::HashMap, io::Write},
};

/// OID for signed attribute containing plist of code directory hashes.
///
/// 1.2.840.113635.100.9.1.
const CDHASH_PLIST_OID: bcder::ConstOid = bcder::Oid(&[42, 134, 72, 134, 247, 99, 100, 9, 1]);

/// Determines whether this crate is capable of signing a given Mach-O binary.
///
/// Code in this crate is limited in the amount of Mach-O binary manipulation
/// it can perform (supporting rewriting all valid Mach-O binaries effectively
/// requires low-level awareness of all Mach-O constructs in order to perform
/// offset manipulation). This function can be used to test signing
/// compatibility.
///
/// We currently only support signing Mach-O files already containing an
/// embedded signature. Often linked binaries automatically contain an embedded
/// signature containing just the code directory (without a cryptographically
/// signed signature), so this limitation hopefully isn't impactful.
pub fn check_signing_capability(macho: &MachO) -> Result<(), AppleCodesignError> {
    match find_signature_data(macho)? {
        Some(signature) => {
            // __LINKEDIT needs to be the final segment so we don't have to rewrite
            // offsets.
            if signature.linkedit_segment_index != macho.segments.len() - 1 {
                Err(AppleCodesignError::LinkeditNotLast)
            // There can be no meaningful data after the signature because we don't
            // know how to rewrite it.
            } else if signature.signature_end_offset != signature.linkedit_segment_data.len() {
                Err(AppleCodesignError::DataAfterSignature)
            } else {
                Ok(())
            }
        }
        None => Err(AppleCodesignError::BinaryNoCodeSignature),
    }
}

/// Obtain the XML plist containing code directory hashes.
///
/// This plist is embedded as a signed attribute in the CMS signature.
pub fn create_code_directory_hashes_plist<'a>(
    code_directories: impl Iterator<Item = &'a CodeDirectoryBlob<'a>>,
    digest_type: DigestType,
) -> Result<Vec<u8>, AppleCodesignError> {
    let hashes = code_directories
        .map(|cd| {
            let blob_data = cd.to_blob_bytes()?;

            let digest = digest_type.digest(&blob_data)?;

            Ok(plist::Value::String(base64::encode(&digest)))
        })
        .collect::<Result<Vec<_>, AppleCodesignError>>()?;

    let mut plist = plist::Dictionary::new();
    plist.insert("cdhashes".to_string(), plist::Value::Array(hashes));

    let mut buffer = Vec::<u8>::new();
    plist::Value::from(plist)
        .to_writer_xml(&mut buffer)
        .map_err(AppleCodesignError::CodeDirectoryPlist)?;

    Ok(buffer)
}

/// Derive a new Mach-O binary with new signature data.
fn create_macho_with_signature(
    macho_data: &[u8],
    macho: &MachO,
    signature_data: &[u8],
) -> Result<Vec<u8>, AppleCodesignError> {
    let existing_signature =
        find_signature_data(macho)?.ok_or(AppleCodesignError::BinaryNoCodeSignature)?;

    // This should have already been called. But we do it again out of paranoia.
    check_signing_capability(macho)?;

    // The assumption made by checking_signing_capability() is that signature data
    // is at the end of the __LINKEDIT segment. So the replacement segment is the
    // existing segment truncated at the signature start followed by the new signature
    // data.
    let new_linkedit_segment_size =
        existing_signature.signature_start_offset + signature_data.len();

    let mut cursor = std::io::Cursor::new(Vec::<u8>::new());

    // Mach-O data structures are variable endian. So use the endian defined
    // by the magic when writing.
    let ctx = parse_magic_and_ctx(&macho_data, 0)?
        .1
        .expect("context should have been parsed before");

    cursor.iowrite_with(macho.header, ctx)?;

    // Following the header are load commands. We need to update load commands
    // to reflect changes to the signature size and __LINKEDIT segment size.
    for load_command in &macho.load_commands {
        let original_command_data =
            &macho_data[load_command.offset..load_command.offset + load_command.command.cmdsize()];

        let written_len = match &load_command.command {
            CommandVariant::CodeSignature(command) => {
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
                    &existing_signature.linkedit_segment_data
                        [0..existing_signature.signature_start_offset],
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
/// It handles parsing and rewriting Mach-O binaries. Under the hood it uses
/// [MachOSignatureBuilder] for signature generation and many of its methods
/// call into that type.
///
/// Signing of both single architecture and fat/universal binaries is supported.
/// Signing settings apply to all binaries within a fat binary.
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
/// Our solution to this problem is to create an intermediate Mach-O binary with
/// placeholder bytes for the signature. We then digest this. When writing
/// the final Mach-O binary we simply replace NULLs with actual signature data,
/// leaving any extra at the end, because truncating the file would require
/// adjusting Mach-O load commands and changing content digests.
#[derive(Debug)]
pub struct MachOSigner<'data, 'key> {
    /// Raw data backing parsed Mach-O binary.
    macho_data: &'data [u8],

    /// Parsed Mach-O binaries.
    machos: Vec<MachO<'data>>,

    /// Mach-O signature builders used by this instance.
    ///
    /// A vector for each binary in a universal binary source.
    signature_builders: Vec<MachOSignatureBuilder<'key>>,
}

impl<'data, 'key> MachOSigner<'data, 'key> {
    /// Construct a new instance from unparsed data representing a Mach-O binary.
    ///
    /// The data will be parsed as a Mach-O binary (either single arch or fat/universal)
    /// and validated that we are capable of signing it.
    pub fn new(macho_data: &'data [u8]) -> Result<Self, AppleCodesignError> {
        let mach = Mach::parse(macho_data)?;

        let (machos, signature_builders) = match mach {
            Mach::Binary(macho) => {
                check_signing_capability(&macho)?;

                (vec![macho], vec![MachOSignatureBuilder::new()?])
            }
            Mach::Fat(multiarch) => {
                let mut machos = vec![];
                let mut builders = vec![];

                for index in 0..multiarch.narches {
                    let macho = multiarch.get(index)?;
                    check_signing_capability(&macho)?;

                    machos.push(macho);
                    builders.push(MachOSignatureBuilder::new()?);
                }

                (machos, builders)
            }
        };

        Ok(Self {
            macho_data,
            machos,
            signature_builders,
        })
    }

    /// See [MachOSignatureBuilder::load_existing_signature_context].
    pub fn load_existing_signature_context(&mut self) -> Result<(), AppleCodesignError> {
        let machos = &self.machos;

        self.signature_builders = self
            .signature_builders
            .drain(..)
            .enumerate()
            .map(|(index, builder)| builder.load_existing_signature_context(&machos[index]))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    /// See [MachOSignatureBuilder::signing_key].
    pub fn signing_key(&mut self, private: &'key SigningKey, public: Certificate) {
        self.signature_builders = self
            .signature_builders
            .drain(..)
            .map(|builder| builder.signing_key(private, public.clone()))
            .collect::<Vec<_>>();
    }

    /// See [MachOSignatureBuilder::set_entitlements_string].
    pub fn set_entitlements_string(&mut self, v: &impl ToString) {
        self.signature_builders = self
            .signature_builders
            .drain(..)
            .map(|builder| builder.set_entitlements_string(v))
            .collect::<Vec<_>>()
    }

    /// See [MachOSignatureBuilder::set_designated_code_requirements].
    pub fn set_designated_code_requirements(
        &mut self,
        reqs: &CodeRequirements,
    ) -> Result<(), AppleCodesignError> {
        self.signature_builders = self
            .signature_builders
            .drain(..)
            .map(|builder| builder.set_designated_code_requirements(reqs))
            .collect::<Result<Vec<_>, AppleCodesignError>>()?;

        Ok(())
    }

    /// See [MachOSignatureBuilder::set_designated_code_requirements_raw].
    pub fn set_designated_code_requirements_raw(&mut self, requirement: &RequirementBlob) {
        self.signature_builders = self
            .signature_builders
            .drain(..)
            .map(|builder| builder.set_designated_code_requirements_raw(requirement))
            .collect::<Vec<_>>()
    }

    /// See [MachOSignatureBuilder::executable_segment_flags].
    pub fn executable_segment_flags(&mut self, flags: ExecutableSegmentFlags) {
        self.signature_builders = self
            .signature_builders
            .drain(..)
            .map(|builder| builder.executable_segment_flags(flags))
            .collect::<Vec<_>>()
    }

    /// See [MachOSignatureBuilder::add_code_signature_flags].
    pub fn add_code_signature_flags(&mut self, flags: CodeSignatureFlags) {
        self.signature_builders = self
            .signature_builders
            .drain(..)
            .map(|builder| builder.add_code_signature_flags(flags))
            .collect::<Vec<_>>()
    }

    /// See [MachOSignatureBuilder::remove_code_signature_flags].
    pub fn remove_code_signature_flags(&mut self, flags: CodeSignatureFlags) {
        self.signature_builders = self
            .signature_builders
            .drain(..)
            .map(|builder| builder.remove_code_signature_flags(flags))
            .collect::<Vec<_>>()
    }

    /// See [MachOSignatureBuilder::info_plist_data].
    pub fn info_plist_data(&mut self, data: &[u8]) -> Result<(), AppleCodesignError> {
        self.signature_builders = self
            .signature_builders
            .drain(..)
            .map(|builder| builder.info_plist_data(data))
            .collect::<Result<Vec<_>, AppleCodesignError>>()?;

        Ok(())
    }

    /// See [MachOSignatureBuilder::code_resources_data].
    pub fn code_resources_data(&mut self, data: &[u8]) -> Result<(), AppleCodesignError> {
        self.signature_builders = self
            .signature_builders
            .drain(..)
            .map(|builder| builder.code_resources_data(data))
            .collect::<Result<Vec<_>, AppleCodesignError>>()?;

        Ok(())
    }

    /// See [MachOSignatureBuilder::chain_certificate_der].
    pub fn chain_certificate_der(
        &mut self,
        data: impl AsRef<[u8]>,
    ) -> Result<(), AppleCodesignError> {
        self.signature_builders = self
            .signature_builders
            .drain(..)
            .map(|builder| builder.chain_certificate_der(data.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    /// See [MachOSignatureBuilder::chain_certificate_pem].
    pub fn chain_certificate_pem(
        &mut self,
        data: impl AsRef<[u8]>,
    ) -> Result<(), AppleCodesignError> {
        self.signature_builders = self
            .signature_builders
            .drain(..)
            .map(|builder| builder.chain_certificate_pem(data.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    /// See [MachOSignatureBuilder::time_stamp_url].
    pub fn time_stamp_url(&mut self, url: impl IntoUrl) -> Result<(), AppleCodesignError> {
        let url = url.into_url()?;

        self.signature_builders = self
            .signature_builders
            .drain(..)
            .map(|builder| builder.time_stamp_url(url.clone()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    /// Write signed Mach-O data to the given writer.
    pub fn write_signed_binary(&self, writer: &mut impl Write) -> Result<(), AppleCodesignError> {
        // Implementing a true streaming writer requires calculating final sizes
        // of all binaries so fat header offsets and sizes can be written first. We take
        // the easy road and buffer individual Mach-O binaries internally.

        let binaries = self
            .signature_builders
            .iter()
            .enumerate()
            .map(|(index, builder)| {
                let original_macho = &self.machos[index];

                // Derive an intermediate Mach-O with placeholder NULLs for signature
                // data so Code Directory digests are correct.
                let placeholder_signature_len = builder.create_superblob(original_macho)?.len();
                let placeholder_signature = b"\0".repeat(placeholder_signature_len + 1024);

                // TODO calling this twice could be undesirable, especially if using
                // a timestamp server. Should we call in no-op mode or write a size
                // estimation function instead?
                let intermediate_macho_data = create_macho_with_signature(
                    self.macho_data(index),
                    original_macho,
                    &placeholder_signature,
                )?;

                // A nice side-effect of this is that it catches bugs if we write malformed Mach-O!
                let intermediate_macho = MachO::parse(&intermediate_macho_data, 0)?;

                let mut signature_data = builder.create_superblob(&intermediate_macho)?;

                // The Mach-O writer adjusts load commands based on the signature length. So pad
                // with NULLs to get to our placeholder length.
                match signature_data.len().cmp(&placeholder_signature.len()) {
                    Ordering::Greater => {
                        return Err(AppleCodesignError::SignatureDataTooLarge);
                    }
                    Ordering::Equal => {}
                    Ordering::Less => {
                        signature_data.extend_from_slice(
                            &b"\0".repeat(placeholder_signature.len() - signature_data.len()),
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

        match Mach::parse(&self.macho_data).expect("should reparse without error") {
            Mach::Binary(_) => {
                assert_eq!(binaries.len(), 1);
                writer.write_all(&binaries[0])?;
            }
            Mach::Fat(multiarch) => {
                assert_eq!(binaries.len(), multiarch.narches);

                // The fat arch header records the start offset and size of each binary.
                // Do a pass over the binaries and calculate these offsets.
                //
                // Binaries appear to be 4k page aligned, so also collect padding
                // information so we write nulls later.
                let mut current_offset = SIZEOF_FAT_HEADER + SIZEOF_FAT_ARCH * binaries.len();
                let mut write_instructions = Vec::with_capacity(binaries.len());

                for (index, arch) in multiarch.iter_arches().enumerate() {
                    let mut arch = arch?;
                    let macho_data = &binaries[index];

                    let pad_bytes = 4096 - current_offset % 4096;

                    arch.offset = (current_offset + pad_bytes) as _;
                    arch.size = macho_data.len() as _;

                    current_offset += macho_data.len() + pad_bytes;

                    write_instructions.push((arch, pad_bytes, macho_data));
                }

                writer.iowrite_with(FAT_MAGIC, scroll::BE)?;
                writer.iowrite_with(multiarch.narches as u32, scroll::BE)?;

                for (fat_arch, _, _) in &write_instructions {
                    let mut buffer = [0u8; SIZEOF_FAT_ARCH];
                    buffer.pwrite_with(fat_arch, 0, scroll::BE)?;
                    writer.write_all(&buffer)?;
                }

                for (_, pad_bytes, macho_data) in write_instructions {
                    writer.write_all(&b"\0".repeat(pad_bytes))?;
                    writer.write_all(macho_data)?;
                }
            }
        }

        Ok(())
    }

    /// Derive the data slice belonging to a Mach-O binary.
    fn macho_data(&self, index: usize) -> &[u8] {
        match Mach::parse(&self.macho_data).expect("should reparse without error") {
            Mach::Binary(_) => &self.macho_data,
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
}

/// Build Apple embedded signatures from parameters.
///
/// This type provides a high-level interface for signing a Mach-O binary.
///
/// You probably want to use [MachOSigner] instead, as it provides more
/// capabilities and is easier to use.
#[derive(Debug)]
pub struct MachOSignatureBuilder<'key> {
    /// Identifier string for the binary.
    ///
    /// This is likely the `CFBundleIdentifier` value from the `Info.plist` in a bundle.
    /// e.g. `com.example.my_program`.
    identifier: Option<String>,

    /// Team name string for the binary.
    team_name: Option<String>,

    /// Digest method to use.
    hash_type: DigestType,

    /// Embedded entitlements data.
    entitlements: Option<EntitlementsBlob<'static>>,

    /// Code requirement data.
    code_requirement: Option<RequirementSetBlob<'static>>,

    /// Setup and mode flags from CodeDirectory.
    cdflags: Option<CodeSignatureFlags>,

    /// Flags for Code Directory execseg field.
    executable_segment_flags: Option<ExecutableSegmentFlags>,

    /// Runtime minimum version requirement.
    ///
    /// Corresponds to `CodeDirectory`'s `runtime` field.
    runtime: Option<u32>,

    /// Digest of `Info.plist` XML plist file.
    info_plist_digest: Option<Digest<'static>>,

    /// Digest of the `CodeResources` XML plist file.
    resources_digest: Option<Digest<'static>>,

    /// The key pair to cryptographically sign with.
    ///
    /// Optional because we can write an embedded signature with just the
    /// code directory without a digital signature of it.
    signing_key: Option<(&'key SigningKey, Certificate)>,

    /// Certificate information to include.
    certificates: Vec<Certificate>,

    /// Time-Stamp Protocol server URL to use.
    time_stamp_url: Option<Url>,
}

impl<'key> MachOSignatureBuilder<'key> {
    /// Create an instance that will sign a MachO binary.
    pub fn new() -> Result<Self, AppleCodesignError> {
        Ok(Self {
            identifier: None,
            team_name: None,
            hash_type: DigestType::Sha256,
            entitlements: None,
            code_requirement: None,
            cdflags: None,
            executable_segment_flags: None,
            runtime: None,
            info_plist_digest: None,
            resources_digest: None,
            signing_key: None,
            certificates: vec![],
            time_stamp_url: None,
        })
    }

    /// Loads context from an existing signature on the binary into this builder.
    ///
    /// By default, newly constructed builders have no context and each field
    /// must be populated manually. When this function is called, existing
    /// signature data in the Mach-O binary will be "imported" to this builder and
    /// settings should be carried forward.
    ///
    /// If the binary has no signature data, this function does nothing.
    pub fn load_existing_signature_context(
        mut self,
        macho: &MachO,
    ) -> Result<Self, AppleCodesignError> {
        if let Some(signature) = find_signature_data(macho)? {
            let signature = EmbeddedSignature::from_bytes(signature.signature_data)?;

            if let Some(cd) = signature.code_directory()? {
                self.identifier = Some(cd.ident.to_string());
                self.team_name = cd.team_name.map(|x| x.to_string());
                self.hash_type = cd.hash_type;
                self.cdflags = Some(cd.flags);
                self.executable_segment_flags = cd.exec_seg_flags;
                self.runtime = cd.runtime;

                if let Some(digest) = cd.special_hashes.get(&CodeSigningSlot::Info) {
                    self.info_plist_digest = Some(digest.to_owned());
                }

                if let Some(digest) = cd.special_hashes.get(&CodeSigningSlot::ResourceDir) {
                    self.resources_digest = Some(digest.to_owned());
                }
            }

            if let Some(blob) = signature.code_requirements()? {
                self.code_requirement = Some(blob.to_owned());
            }

            if let Some(entitlements) = signature.entitlements()? {
                self.entitlements = Some(EntitlementsBlob::from_string(&entitlements));
            }

            Ok(self)
        } else {
            Ok(self)
        }
    }

    /// Set the key to use to create a cryptographic signature.
    ///
    /// If not called, no cryptographic signature will be recorded.
    pub fn signing_key(mut self, private: &'key SigningKey, public: Certificate) -> Self {
        self.signing_key = Some((private, public));

        self
    }

    /// Set the value of the entitlements string to sign.
    ///
    /// This should be an XML plist.
    ///
    /// Accepts any argument that converts to a `String`.
    pub fn set_entitlements_string(mut self, v: &impl ToString) -> Self {
        self.entitlements = Some(EntitlementsBlob::from_string(v));

        self
    }

    /// Set designated code requirements from a [CodeRequirements] instance.
    pub fn set_designated_code_requirements(
        mut self,
        reqs: &CodeRequirements,
    ) -> Result<Self, AppleCodesignError> {
        if self.code_requirement.is_none() {
            self.code_requirement = Some(RequirementSetBlob::default());
        }

        let blob = self.code_requirement.as_mut().unwrap();
        reqs.add_to_requirement_set(blob, RequirementType::Designated)?;

        Ok(self)
    }

    /// Set designated code requirements from raw binary data.
    pub fn set_designated_code_requirements_raw(mut self, requirement: &RequirementBlob) -> Self {
        if self.code_requirement.is_none() {
            self.code_requirement = Some(RequirementSetBlob::default());
        }

        let blob = self.code_requirement.as_mut().unwrap();
        blob.set_requirements(RequirementType::Designated, requirement.to_owned());

        self
    }

    /// Add a [CodeSignatureFlags] to the set of active flags.
    pub fn add_code_signature_flags(mut self, flags: CodeSignatureFlags) -> Self {
        let existing = self.cdflags.unwrap_or_else(CodeSignatureFlags::empty);
        self.cdflags = Some(existing | flags);

        self
    }

    /// Remove a [CodeSignatureFlags] from the set of active flags.
    pub fn remove_code_signature_flags(mut self, flags: CodeSignatureFlags) -> Self {
        let existing = self.cdflags.unwrap_or_else(CodeSignatureFlags::empty);
        self.cdflags = Some(existing - flags);

        self
    }

    /// Set the executable segment flags for this binary.
    pub fn executable_segment_flags(mut self, flags: ExecutableSegmentFlags) -> Self {
        self.executable_segment_flags.replace(flags);

        self
    }

    /// Define the Info.plist content.
    ///
    /// Signatures can reference the digest of an external `Info.plist` file in
    /// the bundle the binary is located in.
    ///
    /// This function tells us what the raw content of that file is so that the
    /// content can be digested and the digest included in the code directory.
    ///
    /// The value passed here should be the raw content of the `Info.plist` XML file.
    pub fn info_plist_data(mut self, data: &[u8]) -> Result<Self, AppleCodesignError> {
        self.info_plist_digest.replace(Digest {
            data: self.hash_type.digest(data)?.into(),
        });

        Ok(self)
    }

    /// Define the code resources content.
    ///
    /// Signatures can reference the digest of an external *Code Resources*
    /// file defining external files and their digests. This file likely exists
    /// as a `_CodeSignature/CodeResources` file inside the bundle.
    ///
    /// This function tells us what the raw content of that file is so that
    /// content can be digested and the digest included in the code directory.
    ///
    /// The value passed here should be the raw content of the XML plist defining
    /// code resources metadata.
    pub fn code_resources_data(mut self, data: &[u8]) -> Result<Self, AppleCodesignError> {
        self.resources_digest.replace(Digest {
            data: self.hash_type.digest(data)?.into(),
        });

        Ok(self)
    }

    /// Add a DER encoded X.509 public certificate to the signing chain.
    ///
    /// Use this to add the raw binary content of an ASN.1 encoded public
    /// certificate.
    ///
    /// The DER data is decoded at function call time. Any error decoding the
    /// certificate will result in `Err`. No validation of the certificate is
    /// performed.
    pub fn chain_certificate_der(
        mut self,
        data: impl AsRef<[u8]>,
    ) -> Result<Self, AppleCodesignError> {
        self.certificates
            .push(Certificate::from_der(data.as_ref())?);

        Ok(self)
    }

    /// Add a PEM encoded X.509 public certificate to the signing chain.
    ///
    /// PEM data looks like `-----BEGIN CERTIFICATE-----` and is a common method
    /// for encoding certificate data. (PEM is effectively base64 encoded DER data.)
    ///
    /// Only a single certificate is read from the PEM data.
    pub fn chain_certificate_pem(
        mut self,
        data: impl AsRef<[u8]>,
    ) -> Result<Self, AppleCodesignError> {
        self.certificates
            .push(Certificate::from_pem(data.as_ref())?);

        Ok(self)
    }

    /// Set the Time-Stamp Protocol server URL to use to generate a Time-Stamp Token.
    ///
    /// When set, the server will be contacted during signing and a Time-Stamp Token will
    /// be embedded in the CMS data structure.
    pub fn time_stamp_url(mut self, url: impl IntoUrl) -> Result<Self, AppleCodesignError> {
        self.time_stamp_url = Some(url.into_url()?);

        Ok(self)
    }

    /// Create data constituting the SuperBlob to be embedded in the `__LINKEDIT` segment.
    ///
    /// The superblob contains the code directory, any extra blobs, and an optional
    /// CMS structure containing a cryptographic signature.
    ///
    /// This takes an explicit Mach-O to operate on due to a circular dependency
    /// between writing out the Mach-O and digesting its content. See the note
    /// in [MachOSigner] for details.
    pub fn create_superblob(&self, macho: &MachO) -> Result<Vec<u8>, AppleCodesignError> {
        let code_directory = self.create_code_directory(macho)?;

        // By convention, the Code Directory goes first.
        let mut blobs = vec![(
            CodeSigningSlot::CodeDirectory,
            code_directory.to_blob_bytes()?,
        )];
        blobs.extend(self.create_special_blobs()?);

        // And the CMS signature goes last.
        if self.signing_key.is_some() {
            blobs.push((
                CodeSigningSlot::Signature,
                BlobWrapperBlob::from_data(&self.create_cms_signature(&code_directory)?)
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
        code_directory: &CodeDirectoryBlob,
    ) -> Result<Vec<u8>, AppleCodesignError> {
        let (signing_key, signing_cert) = self
            .signing_key
            .as_ref()
            .ok_or(AppleCodesignError::NoSigningCertificate)?;

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
                bcder::Oid(Bytes::copy_from_slice(CDHASH_PLIST_OID.as_ref())),
                &code_directory_hashes_plist,
            );
        let signer = if let Some(time_stamp_url) = &self.time_stamp_url {
            signer.time_stamp_url(time_stamp_url.clone())?
        } else {
            signer
        };

        let ber = SignedDataBuilder::default()
            .signer(signer)
            .certificates(self.certificates.iter().cloned())?
            .build_ber()?;

        Ok(ber)
    }

    /// Create the `CodeDirectory` for the current configuration.
    ///
    /// This takes an explicit Mach-O to operate on due to a circular dependency
    /// between writing out the Mach-O and digesting its content. See the note
    /// in [MachOSigner] for details.
    pub fn create_code_directory(
        &self,
        macho: &MachO,
    ) -> Result<CodeDirectoryBlob<'static>, AppleCodesignError> {
        // TODO support defining or filling in proper values for fields with
        // static values.

        let mut flags = self.cdflags.unwrap_or_else(CodeSignatureFlags::empty);

        // The adhoc flag is set when there is no CMS signature.
        if self.signing_key.is_none() {
            flags |= CodeSignatureFlags::ADHOC;
        }

        // Code limit fields hold the file offset at which code digests stop. This
        // is the file offset in the `__LINKEDIT` segment when the embedded signature
        // SuperBlob begins.
        let (code_limit, code_limit_64) = match find_signature_data(macho)? {
            Some(sig) => {
                // If binary already has signature data, take existing signature start offset.
                let limit = sig.linkedit_signature_start_offset;

                if limit > u32::MAX as usize {
                    (0, Some(limit as u64))
                } else {
                    (limit as u32, None)
                }
            }
            None => {
                // No existing signature in binary. Look for __LINKEDIT and use its
                // end offset.
                match macho
                    .segments
                    .iter()
                    .find(|x| matches!(x.name(), Ok("__LINKEDIT")))
                {
                    Some(segment) => {
                        let limit = segment.fileoff as usize + segment.data.len();

                        if limit > u32::MAX as usize {
                            (0, Some(limit as u64))
                        } else {
                            (limit as u32, None)
                        }
                    }
                    None => {
                        let last_segment = macho.segments.iter().last().unwrap();
                        let limit = last_segment.fileoff as usize + last_segment.data.len();

                        if limit > u32::MAX as usize {
                            (0, Some(limit as u64))
                        } else {
                            (limit as u32, None)
                        }
                    }
                }
            }
        };

        let platform = 0;
        let page_size = 4096u32;

        let code_hashes = compute_code_hashes(macho, self.hash_type, Some(page_size as usize))?
            .into_iter()
            .map(|v| Digest { data: v.into() })
            .collect::<Vec<_>>();

        let mut special_hashes = self
            .create_special_blobs()?
            .into_iter()
            .map(|(slot, data)| {
                Ok((
                    slot,
                    Digest {
                        data: self.hash_type.digest(&data)?.into(),
                    },
                ))
            })
            .collect::<Result<HashMap<_, _>, AppleCodesignError>>()?;

        // Add the resources digest, if defined and not a placeholder.
        if let Some(resources_digest) = &self.resources_digest {
            if !resources_digest.is_null() {
                special_hashes.insert(CodeSigningSlot::ResourceDir, resources_digest.to_owned());
            }
        }

        if let Some(info_plist_digest) = &self.info_plist_digest {
            if !info_plist_digest.is_null() {
                special_hashes.insert(CodeSigningSlot::Info, info_plist_digest.to_owned());
            }
        }

        let ident = self
            .identifier
            .clone()
            .ok_or(AppleCodesignError::NoIdentifier)?;

        let team_name = self.team_name.clone().map(|x| x.into());

        let mut cd = CodeDirectoryBlob {
            version: 0,
            flags,
            code_limit,
            hash_size: self.hash_type.hash_len()? as u8,
            hash_type: self.hash_type,
            platform,
            page_size,
            spare2: 0,
            scatter_offset: None,
            spare3: None,
            code_limit_64,
            exec_seg_base: None,
            exec_seg_limit: None,
            exec_seg_flags: self.executable_segment_flags,
            runtime: self.runtime,
            pre_encrypt_offset: None,
            linkage_hash_type: None,
            linkage_truncated: None,
            spare4: None,
            linkage_offset: None,
            linkage_size: None,
            ident: ident.into(),
            team_name,
            code_hashes,
            special_hashes,
        };

        cd.adjust_version();
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
    ) -> Result<Vec<(CodeSigningSlot, Vec<u8>)>, AppleCodesignError> {
        let mut res = Vec::new();

        if let Some(requirements) = &self.code_requirement {
            res.push((
                CodeSigningSlot::RequirementSet,
                requirements.to_blob_bytes()?,
            ));
        }

        if let Some(entitlements) = &self.entitlements {
            res.push((CodeSigningSlot::Entitlements, entitlements.to_blob_bytes()?));
        }

        Ok(res)
    }
}
