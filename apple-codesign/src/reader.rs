// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality for reading signature data from files.

use {
    crate::{
        certificate::AppleCertificate,
        code_directory::CodeDirectoryBlob,
        dmg::{path_is_dmg, DmgReader},
        embedded_signature::{BlobEntry, DigestType, EmbeddedSignature},
        embedded_signature_builder::{CD_DIGESTS_OID, CD_DIGESTS_PLIST_OID},
        error::AppleCodesignError,
        macho::{get_macho_from_data, AppleSignable},
    },
    apple_bundles::{DirectoryBundle, DirectoryBundleFile},
    apple_xar::{
        reader::XarReader,
        table_of_contents::{
            ChecksumType as XarChecksumType, File as XarTocFile, Signature as XarTocSignature,
        },
    },
    cryptographic_message_syntax::{SignedData, SignerInfo},
    goblin::mach::{fat::FAT_MAGIC, parse_magic_and_ctx, Mach, MachO},
    serde::Serialize,
    std::{
        fmt::Debug,
        fs::File,
        io::{BufWriter, Cursor, Read, Seek},
        ops::Deref,
        path::{Path, PathBuf},
    },
    x509_certificate::{CapturedX509Certificate, DigestAlgorithm},
};

enum MachOType {
    Mach,
    MachO,
}

impl MachOType {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Option<Self>, AppleCodesignError> {
        let mut fh = File::open(path.as_ref())?;

        let mut header = vec![0u8; 4];
        let count = fh.read(&mut header)?;

        if count < 4 {
            return Ok(None);
        }

        let magic = goblin::mach::peek(&header, 0)?;

        match magic {
            FAT_MAGIC => Ok(Some(Self::Mach)),
            _ if parse_magic_and_ctx(&header, 0).is_ok() => Ok(Some(Self::MachO)),
            _ => Ok(None),
        }
    }
}

/// Test whether a given path is likely a XAR file.
pub fn path_is_xar(path: impl AsRef<Path>) -> Result<bool, AppleCodesignError> {
    let mut fh = File::open(path.as_ref())?;

    let mut header = [0u8; 4];

    let count = fh.read(&mut header)?;
    if count < 4 {
        Ok(false)
    } else {
        Ok(header.as_ref() == b"xar!")
    }
}

/// Describes the type of entity at a path.
///
/// This represents a best guess.
pub enum PathType {
    MachO,
    Dmg,
    Bundle,
    Xar,
    Other,
}

impl PathType {
    /// Attempt to classify the type of signable entity based on a filesystem path.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, AppleCodesignError> {
        let path = path.as_ref();

        if path.is_file() {
            if path_is_dmg(path)? {
                Ok(PathType::Dmg)
            } else if path_is_xar(path)? {
                Ok(PathType::Xar)
            } else {
                match MachOType::from_path(path)? {
                    Some(MachOType::Mach | MachOType::MachO) => Ok(Self::MachO),
                    None => Ok(Self::Other),
                }
            }
        } else if path.is_dir() {
            Ok(PathType::Bundle)
        } else {
            Ok(PathType::Other)
        }
    }
}

fn pretty_print_xml(xml: &[u8]) -> Result<Vec<u8>, AppleCodesignError> {
    let mut reader = xml::reader::EventReader::new(Cursor::new(xml));
    let mut emitter = xml::EmitterConfig::new()
        .perform_indent(true)
        .create_writer(BufWriter::new(Vec::with_capacity(xml.len() * 2)));

    while let Ok(event) = reader.next() {
        match event {
            xml::reader::XmlEvent::EndDocument => {
                break;
            }
            xml::reader::XmlEvent::Whitespace(_) => {}
            event => {
                if let Some(event) = event.as_writer_event() {
                    emitter.write(event).map_err(AppleCodesignError::XmlWrite)?;
                }
            }
        }
    }

    let xml = emitter.into_inner().into_inner().map_err(|e| {
        AppleCodesignError::Io(std::io::Error::new(std::io::ErrorKind::BrokenPipe, e))
    })?;

    Ok(xml)
}

#[derive(Clone, Debug, Serialize)]
pub struct BlobDescription {
    pub slot: String,
    pub magic: String,
    pub length: u32,
    pub sha1: String,
    pub sha256: String,
}

impl<'a> From<&BlobEntry<'a>> for BlobDescription {
    fn from(entry: &BlobEntry<'a>) -> Self {
        Self {
            slot: format!("{:?}", entry.slot),
            magic: format!("{:x}", u32::from(entry.magic)),
            length: entry.length as _,
            sha1: hex::encode(
                entry
                    .digest_with(DigestType::Sha1)
                    .expect("sha-1 digest should always work"),
            ),
            sha256: hex::encode(
                entry
                    .digest_with(DigestType::Sha256)
                    .expect("sha-256 digest should always work"),
            ),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CertificateInfo {
    pub subject: String,
    pub issuer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_algorithm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_algorithm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_with_algorithm: Option<String>,
    pub is_apple_root_ca: bool,
    pub is_apple_intermediate_ca: bool,
    pub chains_to_apple_root_ca: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apple_ca_extension: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub apple_extended_key_usages: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub apple_code_signing_extensions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apple_certificate_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apple_team_id: Option<String>,
}

impl TryFrom<&CapturedX509Certificate> for CertificateInfo {
    type Error = AppleCodesignError;

    fn try_from(cert: &CapturedX509Certificate) -> Result<Self, Self::Error> {
        Ok(Self {
            subject: cert
                .subject_name()
                .user_friendly_str()
                .map_err(AppleCodesignError::CertificateDecode)?,
            issuer: cert
                .issuer_name()
                .user_friendly_str()
                .map_err(AppleCodesignError::CertificateDecode)?,
            key_algorithm: cert.key_algorithm().map(|x| x.to_string()),
            signature_algorithm: cert.signature_algorithm().map(|x| x.to_string()),
            signed_with_algorithm: cert.signature_signature_algorithm().map(|x| x.to_string()),
            is_apple_root_ca: cert.is_apple_root_ca(),
            is_apple_intermediate_ca: cert.is_apple_intermediate_ca(),
            chains_to_apple_root_ca: cert.chains_to_apple_root_ca(),
            apple_ca_extension: cert.apple_ca_extension().map(|x| x.to_string()),
            apple_extended_key_usages: cert
                .apple_extended_key_usage_purposes()
                .into_iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>(),
            apple_code_signing_extensions: cert
                .apple_code_signing_extensions()
                .into_iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>(),
            apple_certificate_profile: cert.apple_guess_profile().map(|x| x.to_string()),
            apple_team_id: cert.apple_team_id(),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CmsSigner {
    pub issuer: String,
    pub digest_algorithm: String,
    pub signature_algorithm: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_time: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cdhash_plist: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cdhash_digests: Vec<(String, String)>,
    pub signature_verifies: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_stamp_token: Option<CmsSignature>,
}

impl CmsSigner {
    pub fn from_signer_info_and_signed_data(
        signer_info: &SignerInfo,
        signed_data: &SignedData,
    ) -> Result<Self, AppleCodesignError> {
        let mut attributes = vec![];
        let mut content_type = None;
        let mut message_digest = None;
        let mut signing_time = None;
        let mut time_stamp_token = None;
        let mut cdhash_plist = vec![];
        let mut cdhash_digests = vec![];

        if let Some(sa) = signer_info.signed_attributes() {
            content_type = Some(sa.content_type().to_string());
            message_digest = Some(hex::encode(sa.message_digest()));
            if let Some(t) = sa.signing_time() {
                signing_time = Some(*t);
            }

            for attr in sa.attributes().iter() {
                attributes.push(format!("{}", attr.typ));

                if attr.typ == CD_DIGESTS_PLIST_OID {
                    if let Some(data) = attr.values.get(0) {
                        let data = data.deref().clone();

                        let plist = data
                            .decode(|cons| {
                                let v = bcder::OctetString::take_from(cons)?;

                                Ok(v.into_bytes())
                            })
                            .map_err(|e| AppleCodesignError::Cms(e.into()))?;

                        cdhash_plist = String::from_utf8_lossy(&pretty_print_xml(&plist)?)
                            .lines()
                            .map(|x| x.to_string())
                            .collect::<Vec<_>>();
                    }
                } else if attr.typ == CD_DIGESTS_OID {
                    for value in &attr.values {
                        // Each value is a SEQUENECE of (OID, OctetString).
                        let data = value.deref().clone();

                        data.decode(|cons| {
                            while let Some(_) = cons.take_opt_sequence(|cons| {
                                let oid = bcder::Oid::take_from(cons)?;
                                let value = bcder::OctetString::take_from(cons)?;

                                cdhash_digests
                                    .push((format!("{}", oid), hex::encode(value.into_bytes())));

                                Ok(())
                            })? {}

                            Ok(())
                        })
                        .map_err(|e| AppleCodesignError::Cms(e.into()))?;
                    }
                }
            }
        }

        // The order should matter per RFC 5652 but Apple's CMS implementation doesn't
        // conform to spec.
        attributes.sort();

        if let Some(tsk) = signer_info.time_stamp_token_signed_data()? {
            time_stamp_token = Some(tsk.try_into()?);
        }

        Ok(Self {
            issuer: signer_info
                .certificate_issuer_and_serial()
                .expect("issuer should always be set")
                .0
                .user_friendly_str()
                .map_err(AppleCodesignError::CertificateDecode)?,
            digest_algorithm: signer_info.digest_algorithm().to_string(),
            signature_algorithm: signer_info.signature_algorithm().to_string(),
            attributes,
            content_type,
            message_digest,
            signing_time,
            cdhash_plist,
            cdhash_digests,
            signature_verifies: signer_info
                .verify_signature_with_signed_data(signed_data)
                .is_ok(),

            time_stamp_token,
        })
    }
}

/// High-level representation of a CMS signature.
#[derive(Clone, Debug, Serialize)]
pub struct CmsSignature {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub certificates: Vec<CertificateInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub signers: Vec<CmsSigner>,
}

impl TryFrom<SignedData> for CmsSignature {
    type Error = AppleCodesignError;

    fn try_from(signed_data: SignedData) -> Result<Self, Self::Error> {
        let certificates = signed_data
            .certificates()
            .map(|x| x.try_into())
            .collect::<Result<Vec<_>, _>>()?;

        let signers = signed_data
            .signers()
            .map(|x| CmsSigner::from_signer_info_and_signed_data(x, &signed_data))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            certificates,
            signers,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CodeDirectory {
    pub version: String,
    pub flags: String,
    pub identifier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_name: Option<String>,
    pub digest_type: String,
    pub platform: u8,
    pub signed_entity_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_segment_flags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_version: Option<String>,
    pub code_digests_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    slot_digests: Vec<String>,
}

impl<'a> TryFrom<CodeDirectoryBlob<'a>> for CodeDirectory {
    type Error = AppleCodesignError;

    fn try_from(cd: CodeDirectoryBlob<'a>) -> Result<Self, Self::Error> {
        let mut temp = cd
            .slot_digests()
            .into_iter()
            .map(|(slot, digest)| (slot, digest.as_hex()))
            .collect::<Vec<_>>();
        temp.sort_by(|(a, _), (b, _)| a.cmp(b));

        let slot_digests = temp
            .into_iter()
            .map(|(slot, digest)| format!("{:?}: {}", slot, digest))
            .collect::<Vec<_>>();

        Ok(Self {
            version: format!("0x{:X}", cd.version),
            flags: format!("{:?}", cd.flags),
            identifier: cd.ident.to_string(),
            team_name: cd.team_name.map(|x| x.to_string()),
            signed_entity_size: cd.code_limit as _,
            digest_type: format!("{}", cd.digest_type),
            platform: cd.platform,
            executable_segment_flags: cd.exec_seg_flags.map(|x| format!("{:?}", x)),
            runtime_version: cd
                .runtime
                .map(|x| format!("{}", crate::macho::parse_version_nibbles(x))),
            code_digests_count: cd.code_digests.len(),
            slot_digests,
        })
    }
}

/// High level representation of a code signature.
#[derive(Clone, Debug, Serialize)]
pub struct CodeSignature {
    /// Length of the code signature data.
    pub superblob_length: u32,
    pub blob_count: u32,
    pub blobs: Vec<BlobDescription>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_directory: Option<CodeDirectory>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alternative_code_directories: Vec<(String, CodeDirectory)>,
    pub entitlements_plist: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub code_requirements: Vec<String>,
    pub cms: Option<CmsSignature>,
}

impl<'a> TryFrom<EmbeddedSignature<'a>> for CodeSignature {
    type Error = AppleCodesignError;

    fn try_from(sig: EmbeddedSignature<'a>) -> Result<Self, Self::Error> {
        let mut entitlements_plist = None;
        let mut code_requirements = vec![];
        let mut cms = None;

        let code_directory = if let Some(cd) = sig.code_directory()? {
            Some(CodeDirectory::try_from(*cd)?)
        } else {
            None
        };

        let alternative_code_directories = sig
            .alternate_code_directories()?
            .into_iter()
            .map(|(slot, cd)| Ok((format!("{:?}", slot), CodeDirectory::try_from(*cd)?)))
            .collect::<Result<Vec<_>, AppleCodesignError>>()?;

        if let Some(blob) = sig.entitlements()? {
            entitlements_plist = Some(blob.as_str().to_string());
        }

        if let Some(req) = sig.code_requirements()? {
            let mut temp = vec![];

            for (req, blob) in req.requirements {
                let reqs = blob.parse_expressions()?;
                temp.push((req, format!("{}", reqs)));
            }

            temp.sort_by(|(a, _), (b, _)| a.cmp(b));

            code_requirements = temp
                .into_iter()
                .map(|(req, value)| format!("{}: {}", req, value))
                .collect::<Vec<_>>();
        }

        if let Some(signed_data) = sig.signed_data()? {
            cms = Some(signed_data.try_into()?);
        }

        Ok(Self {
            superblob_length: sig.length,
            blob_count: sig.count,
            blobs: sig
                .blobs
                .iter()
                .map(BlobDescription::from)
                .collect::<Vec<_>>(),
            code_directory,
            alternative_code_directories,
            entitlements_plist,
            code_requirements,
            cms,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MachOEntity {
    pub signature: Option<CodeSignature>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DmgEntity {
    pub code_signature_offset: u64,
    pub code_signature_size: u64,
    pub signature: Option<CodeSignature>,
}

#[derive(Clone, Debug, Serialize)]
pub enum CodeSignatureFile {
    ResourcesXml(Vec<String>),
    NotarizationTicket,
    Other,
}

#[derive(Clone, Debug, Serialize)]
pub struct XarTableOfContents {
    pub toc_length_compressed: u64,
    pub toc_length_uncompressed: u64,
    pub checksum_offset: u64,
    pub checksum_size: u64,
    pub checksum_type: String,
    pub toc_start_offset: u16,
    pub heap_start_offset: u64,
    pub creation_time: String,
    pub toc_checksum_reported: String,
    pub toc_checksum_reported_sha1_digest: String,
    pub toc_checksum_reported_sha256_digest: String,
    pub toc_checksum_actual_sha1: String,
    pub toc_checksum_actual_sha256: String,
    pub checksum_verifies: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<XarSignature>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_signature: Option<XarSignature>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub xml: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rsa_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rsa_signature_verifies: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cms_signature: Option<CmsSignature>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cms_signature_verifies: Option<bool>,
}

impl XarTableOfContents {
    pub fn from_xar<R: Read + Seek + Sized + Debug>(
        xar: &mut XarReader<R>,
    ) -> Result<Self, AppleCodesignError> {
        let (digest_type, digest) = xar.checksum()?;
        let _xml = xar.table_of_contents_decoded_data()?;

        let (rsa_signature, rsa_signature_verifies) = if let Some(sig) = xar.rsa_signature()? {
            (
                Some(hex::encode(&sig.0)),
                Some(xar.verify_rsa_checksum_signature().unwrap_or(false)),
            )
        } else {
            (None, None)
        };
        let (cms_signature, cms_signature_verifies) =
            if let Some(signed_data) = xar.cms_signature()? {
                (
                    Some(CmsSignature::try_from(signed_data)?),
                    Some(xar.verify_cms_signature().unwrap_or(false)),
                )
            } else {
                (None, None)
            };

        let toc_checksum_actual_sha1 = xar.digest_table_of_contents_with(XarChecksumType::Sha1)?;
        let toc_checksum_actual_sha256 =
            xar.digest_table_of_contents_with(XarChecksumType::Sha256)?;

        let checksum_verifies = xar.verify_table_of_contents_checksum().unwrap_or(false);

        let header = xar.header();
        let toc = xar.table_of_contents();
        let checksum_offset = toc.checksum.offset;
        let checksum_size = toc.checksum.size;

        // This can be useful for debugging.
        //let xml = String::from_utf8_lossy(&pretty_print_xml(&xml)?)
        //    .lines()
        //    .map(|x| x.to_string())
        //    .collect::<Vec<_>>();
        let xml = vec![];

        Ok(Self {
            toc_length_compressed: header.toc_length_compressed,
            toc_length_uncompressed: header.toc_length_uncompressed,
            checksum_offset,
            checksum_size,
            checksum_type: apple_xar::format::XarChecksum::from(header.checksum_algorithm_id)
                .to_string(),
            toc_start_offset: header.size,
            heap_start_offset: xar.heap_start_offset(),
            creation_time: toc.creation_time.clone(),
            toc_checksum_reported: format!("{}:{}", digest_type, hex::encode(&digest)),
            toc_checksum_reported_sha1_digest: hex::encode(DigestType::Sha1.digest_data(&digest)?),
            toc_checksum_reported_sha256_digest: hex::encode(
                DigestType::Sha256.digest_data(&digest)?,
            ),
            toc_checksum_actual_sha1: hex::encode(&toc_checksum_actual_sha1),
            toc_checksum_actual_sha256: hex::encode(&toc_checksum_actual_sha256),
            checksum_verifies,
            signature: if let Some(sig) = &toc.signature {
                Some(sig.try_into()?)
            } else {
                None
            },
            x_signature: if let Some(sig) = &toc.x_signature {
                Some(sig.try_into()?)
            } else {
                None
            },
            xml,
            rsa_signature,
            rsa_signature_verifies,
            cms_signature,
            cms_signature_verifies,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct XarSignature {
    pub style: String,
    pub offset: u64,
    pub size: u64,
    pub end_offset: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub certificates: Vec<CertificateInfo>,
}

impl TryFrom<&XarTocSignature> for XarSignature {
    type Error = AppleCodesignError;

    fn try_from(sig: &XarTocSignature) -> Result<Self, Self::Error> {
        Ok(Self {
            style: sig.style.to_string(),
            offset: sig.offset,
            size: sig.size,
            end_offset: sig.offset + sig.size,
            certificates: sig
                .x509_certificates()?
                .into_iter()
                .map(|cert| CertificateInfo::try_from(&cert))
                .collect::<Result<Vec<_>, AppleCodesignError>>()?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct XarFile {
    pub id: u64,
    pub file_type: String,
    pub data_size: Option<u64>,
    pub data_length: Option<u64>,
    pub data_extracted_checksum: Option<String>,
    pub data_archived_checksum: Option<String>,
    pub data_encoding: Option<String>,
}

impl TryFrom<&XarTocFile> for XarFile {
    type Error = AppleCodesignError;

    fn try_from(file: &XarTocFile) -> Result<Self, Self::Error> {
        let mut v = Self {
            id: file.id,
            file_type: file.file_type.to_string(),
            ..Default::default()
        };

        if let Some(data) = &file.data {
            v.populate_data(data);
        }

        Ok(v)
    }
}

impl XarFile {
    pub fn populate_data(&mut self, data: &apple_xar::table_of_contents::FileData) {
        self.data_size = Some(data.size);
        self.data_length = Some(data.length);
        self.data_extracted_checksum = Some(format!(
            "{}:{}",
            data.extracted_checksum.style, data.extracted_checksum.checksum
        ));
        self.data_archived_checksum = Some(format!(
            "{}:{}",
            data.archived_checksum.style, data.archived_checksum.checksum
        ));
        self.data_encoding = Some(data.encoding.style.clone());
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureEntity {
    MachO(MachOEntity),
    Dmg(DmgEntity),
    BundleCodeSignatureFile(CodeSignatureFile),
    XarTableOfContents(XarTableOfContents),
    XarMember(XarFile),
    Other,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileEntity {
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_path: Option<String>,
    pub entity: SignatureEntity,
}

impl FileEntity {
    /// Construct an instance from a [Path].
    pub fn from_path(path: &Path, report_path: Option<&Path>) -> Result<Self, AppleCodesignError> {
        let metadata = std::fs::symlink_metadata(path)?;

        let report_path = if let Some(p) = report_path {
            p.to_path_buf()
        } else {
            path.to_path_buf()
        };

        let (file_size, file_sha256, symlink_target) = if metadata.is_symlink() {
            (None, None, Some(std::fs::read_link(path)?))
        } else {
            (
                Some(metadata.len()),
                Some(hex::encode(DigestAlgorithm::Sha256.digest_path(path)?)),
                None,
            )
        };

        Ok(Self {
            path: report_path,
            file_size,
            file_sha256,
            symlink_target,
            sub_path: None,
            entity: SignatureEntity::Other,
        })
    }
}

/// Entity for reading Apple code signature data.
pub enum SignatureReader {
    Dmg(PathBuf, Box<DmgReader>),
    MachO(PathBuf, Vec<u8>),
    Bundle(Box<DirectoryBundle>),
    FlatPackage(PathBuf),
}

impl SignatureReader {
    /// Construct a signature reader from a path.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, AppleCodesignError> {
        let path = path.as_ref();
        match PathType::from_path(path)? {
            PathType::Bundle => Ok(Self::Bundle(Box::new(
                DirectoryBundle::new_from_path(path)
                    .map_err(AppleCodesignError::DirectoryBundle)?,
            ))),
            PathType::Dmg => {
                let mut fh = File::open(path)?;
                Ok(Self::Dmg(
                    path.to_path_buf(),
                    Box::new(DmgReader::new(&mut fh)?),
                ))
            }
            PathType::MachO => {
                let data = std::fs::read(path)?;
                get_macho_from_data(&data, 0)?;

                Ok(Self::MachO(path.to_path_buf(), data))
            }
            PathType::Xar => Ok(Self::FlatPackage(path.to_path_buf())),
            PathType::Other => Err(AppleCodesignError::UnrecognizedPathType),
        }
    }

    /// Obtain entities that are possibly relevant to code signing.
    pub fn entities(&self) -> Result<Vec<FileEntity>, AppleCodesignError> {
        match self {
            Self::Dmg(path, dmg) => {
                let mut entity = FileEntity::from_path(path, None)?;
                entity.entity = SignatureEntity::Dmg(Self::resolve_dmg_entity(dmg)?);

                Ok(vec![entity])
            }
            Self::MachO(path, data) => Self::resolve_macho_entities_from_data(path, data, None),
            Self::Bundle(bundle) => Self::resolve_bundle_entities(bundle),
            Self::FlatPackage(path) => Self::resolve_flat_package_entities(path),
        }
    }

    fn resolve_dmg_entity(dmg: &DmgReader) -> Result<DmgEntity, AppleCodesignError> {
        let signature = if let Some(sig) = dmg.embedded_signature()? {
            Some(sig.try_into()?)
        } else {
            None
        };

        Ok(DmgEntity {
            code_signature_offset: dmg.koly().code_signature_offset,
            code_signature_size: dmg.koly().code_signature_size,
            signature,
        })
    }

    fn resolve_macho_entities_from_data(
        path: &Path,
        data: &[u8],
        report_path: Option<&Path>,
    ) -> Result<Vec<FileEntity>, AppleCodesignError> {
        let mut entity = FileEntity::from_path(path, report_path)?;

        match Mach::parse(data)? {
            Mach::Binary(macho) => {
                entity.entity = SignatureEntity::MachO(Self::resolve_macho_entity(macho)?);

                Ok(vec![entity])
            }
            Mach::Fat(multiarch) => {
                let mut entities = vec![];

                for index in 0..multiarch.narches {
                    let macho = multiarch.get(index)?;

                    let mut entity = entity.clone();
                    entity.sub_path = Some(format!("macho-index:{}", index));
                    entity.entity = SignatureEntity::MachO(Self::resolve_macho_entity(macho)?);

                    entities.push(entity);
                }

                Ok(entities)
            }
        }
    }

    fn resolve_macho_entity(macho: MachO) -> Result<MachOEntity, AppleCodesignError> {
        let signature = if let Some(sig) = macho.code_signature()? {
            Some(sig.try_into()?)
        } else {
            None
        };

        Ok(MachOEntity { signature })
    }

    fn resolve_bundle_entities(
        bundle: &DirectoryBundle,
    ) -> Result<Vec<FileEntity>, AppleCodesignError> {
        let mut entities = vec![];

        for file in bundle
            .files(true)
            .map_err(AppleCodesignError::DirectoryBundle)?
        {
            entities.extend(
                Self::resolve_bundle_file_entity(bundle.root_dir().to_path_buf(), file)?
                    .into_iter(),
            );
        }

        Ok(entities)
    }

    fn resolve_bundle_file_entity(
        base_path: PathBuf,
        file: DirectoryBundleFile,
    ) -> Result<Vec<FileEntity>, AppleCodesignError> {
        let main_relative_path = match file.absolute_path().strip_prefix(&base_path) {
            Ok(path) => path.to_path_buf(),
            Err(_) => file.absolute_path().to_path_buf(),
        };

        let mut entities = vec![];

        let mut default_entity =
            FileEntity::from_path(file.absolute_path(), Some(&main_relative_path))?;

        let file_name = file
            .absolute_path()
            .file_name()
            .expect("path should have file name")
            .to_string_lossy();
        let parent_dir = file
            .absolute_path()
            .parent()
            .expect("path should have parent directory");

        // There may be bugs in the code identifying the role of files in bundles.
        // So rely on our own heuristics to detect and report on the file type.
        if default_entity.symlink_target.is_some() {
            entities.push(default_entity);
        } else if parent_dir.ends_with("_CodeSignature") {
            if file_name == "CodeResources" {
                let data = std::fs::read(file.absolute_path())?;

                default_entity.entity =
                    SignatureEntity::BundleCodeSignatureFile(CodeSignatureFile::ResourcesXml(
                        String::from_utf8_lossy(&data)
                            .split('\n')
                            .map(|x| x.replace('\t', "  "))
                            .collect::<Vec<_>>(),
                    ));

                entities.push(default_entity);
            } else {
                default_entity.entity =
                    SignatureEntity::BundleCodeSignatureFile(CodeSignatureFile::Other);

                entities.push(default_entity);
            }
        } else if file_name == "CodeResources" {
            default_entity.entity =
                SignatureEntity::BundleCodeSignatureFile(CodeSignatureFile::NotarizationTicket);

            entities.push(default_entity);
        } else {
            let data = std::fs::read(file.absolute_path())?;

            match Self::resolve_macho_entities_from_data(
                file.absolute_path(),
                &data,
                Some(&main_relative_path),
            ) {
                Ok(extra) => {
                    entities.extend(extra);
                }
                Err(_) => {
                    // Just some extra file.
                    entities.push(default_entity);
                }
            }
        }

        Ok(entities)
    }

    fn resolve_flat_package_entities(path: &Path) -> Result<Vec<FileEntity>, AppleCodesignError> {
        let mut xar = XarReader::new(File::open(path)?)?;

        let default_entity = FileEntity::from_path(path, None)?;

        let mut entities = vec![];

        let mut entity = default_entity.clone();
        entity.sub_path = Some("toc".to_string());
        entity.entity =
            SignatureEntity::XarTableOfContents(XarTableOfContents::from_xar(&mut xar)?);
        entities.push(entity);

        // Now emit entries for all files in table of contents.
        for (name, file) in xar.files()? {
            let mut entity = default_entity.clone();
            entity.sub_path = Some(name);
            entity.entity = SignatureEntity::XarMember(XarFile::try_from(&file)?);
            entities.push(entity);
        }

        Ok(entities)
    }
}
