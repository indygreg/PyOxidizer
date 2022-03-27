// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality for reading signature data from files.

use {
    crate::{
        certificate::AppleCertificate,
        dmg::{path_is_dmg, DmgReader},
        embedded_signature::{BlobEntry, DigestType, EmbeddedSignature},
        error::AppleCodesignError,
        macho::AppleSignable,
    },
    apple_bundles::{DirectoryBundle, DirectoryBundleFile},
    apple_xar::reader::XarReader,
    cryptographic_message_syntax::{SignedData, SignerInfo},
    goblin::mach::{fat::FAT_MAGIC, parse_magic_and_ctx, Mach, MachO},
    serde::Serialize,
    std::{
        fs::File,
        io::Read,
        path::{Path, PathBuf},
    },
    x509_certificate::CapturedX509Certificate,
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

#[derive(Clone, Debug, Serialize)]
pub struct BlobDescription {
    pub slot: String,
    pub magic: String,
    pub length: u32,
    pub sha256: String,
}

impl<'a> From<&BlobEntry<'a>> for BlobDescription {
    fn from(entry: &BlobEntry<'a>) -> Self {
        Self {
            slot: format!("{:?}", entry.slot),
            magic: format!("{:x}", u32::from(entry.magic)),
            length: entry.length as _,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_time: Option<chrono::DateTime<chrono::Utc>>,
    pub signature_verifies: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_stamp_token: Option<CmsSignature>,
}

impl CmsSigner {
    pub fn from_signer_info_and_signed_data(
        signer_info: &SignerInfo,
        signed_data: &SignedData,
    ) -> Result<Self, AppleCodesignError> {
        let mut content_type = None;
        let mut message_digest = None;
        let mut signing_time = None;
        let mut time_stamp_token = None;

        if let Some(sa) = signer_info.signed_attributes() {
            content_type = Some(sa.content_type().to_string());
            message_digest = Some(hex::encode(sa.message_digest()));
            if let Some(t) = sa.signing_time() {
                signing_time = Some(*t);
            }
        }

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
            content_type,
            message_digest,
            signing_time,
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

/// High level representation of a code signature.
#[derive(Clone, Debug, Serialize)]
pub struct CodeSignature {
    /// Length of the code signature data.
    pub superblob_length: u32,
    pub blob_count: u32,
    pub blobs: Vec<BlobDescription>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_entity_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_segment_flags: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub slot_digests: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entitlements_plist: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub code_requirements: Vec<String>,
    pub cms: Option<CmsSignature>,
}

impl<'a> TryFrom<EmbeddedSignature<'a>> for CodeSignature {
    type Error = AppleCodesignError;

    fn try_from(sig: EmbeddedSignature<'a>) -> Result<Self, Self::Error> {
        let mut version = None;
        let mut flags = None;
        let mut identifier = None;
        let mut team_name = None;
        let mut signed_entity_size = None;
        let mut digest_type = None;
        let mut executable_segment_flags = None;
        let mut slot_digests = vec![];
        let mut entitlements_plist = None;
        let mut code_requirements = vec![];
        let mut cms = None;

        if let Some(cd) = sig.code_directory()? {
            version = Some(cd.version);
            flags = Some(format!("{:?}", cd.flags));
            identifier = Some(cd.ident.to_string());
            team_name = cd.team_name.map(|x| x.to_string());
            signed_entity_size = Some(cd.code_limit as _);
            digest_type = Some(format!("{}", cd.hash_type));
            executable_segment_flags = cd.exec_seg_flags.map(|x| format!("{:?}", x));

            let mut temp = cd
                .special_hashes
                .into_iter()
                .map(|(slot, digest)| (slot, digest.as_hex()))
                .collect::<Vec<_>>();
            temp.sort_by(|(a, _), (b, _)| a.cmp(b));

            slot_digests = temp
                .into_iter()
                .map(|(slot, digest)| format!("{:?}: {}", slot, digest))
                .collect::<Vec<_>>();
        }

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
            version,
            flags,
            identifier,
            team_name,
            digest_type,
            signed_entity_size,
            executable_segment_flags,
            slot_digests,
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
    NotarizationTicket(u64),
    Other,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureEntity {
    MachO(MachOEntity),
    Dmg(DmgEntity),
    BundleCodeSignatureFile(CodeSignatureFile),
}

#[derive(Clone, Debug, Serialize)]
pub struct FileEntity {
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_path: Option<String>,
    pub entity: SignatureEntity,
}

/// Entity for reading Apple code signature data.
pub enum SignatureReader {
    Dmg(PathBuf, Box<DmgReader>),
    MachO(PathBuf, Vec<u8>),
    Bundle(Box<DirectoryBundle>),
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
                Mach::parse(&data)?;

                Ok(Self::MachO(path.to_path_buf(), data))
            }
            PathType::Xar => {
                XarReader::new(File::open(path)?)?;

                Err(AppleCodesignError::Unimplemented("XAR signature reading"))
            }
            PathType::Other => Err(AppleCodesignError::UnrecognizedPathType),
        }
    }

    /// Iterate over entities related to code signing.
    pub fn iter_entities(
        &self,
    ) -> Box<dyn Iterator<Item = Result<FileEntity, AppleCodesignError>> + '_> {
        match self {
            Self::Dmg(path, dmg) => Box::new(std::iter::once(Self::resolve_dmg_entity(dmg).map(
                |entity| FileEntity {
                    path: path.to_path_buf(),
                    sub_path: None,
                    entity: SignatureEntity::Dmg(entity),
                },
            ))),
            Self::MachO(path, data) => Self::resolve_macho_entities_from_data(path, data),
            Self::Bundle(bundle) => Self::resolve_bundle_entities(bundle),
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

    fn resolve_macho_entities_from_data<'a>(
        path: &'a Path,
        data: &'a [u8],
    ) -> Box<dyn Iterator<Item = Result<FileEntity, AppleCodesignError>> + 'a> {
        match Mach::parse(data) {
            Ok(mach) => match mach {
                Mach::Binary(macho) => Box::new(std::iter::once(
                    Self::resolve_macho_entity(macho).map(|entity| FileEntity {
                        path: path.to_path_buf(),
                        sub_path: None,
                        entity: SignatureEntity::MachO(entity),
                    }),
                )),
                Mach::Fat(multiarch) => {
                    Box::new(
                        (0..multiarch.narches).map(move |index| match multiarch.get(index) {
                            Ok(macho) => {
                                Self::resolve_macho_entity(macho).map(|entity| FileEntity {
                                    path: path.to_path_buf(),
                                    sub_path: Some(format!("macho-index:{}", index)),
                                    entity: SignatureEntity::MachO(entity),
                                })
                            }
                            Err(err) => Err(err.into()),
                        }),
                    )
                }
            },
            Err(err) => Box::new(std::iter::once(Err(err.into()))),
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
    ) -> Box<dyn Iterator<Item = Result<FileEntity, AppleCodesignError>> + '_> {
        match bundle.files(true) {
            Ok(files) => Box::new(files.into_iter().flat_map(|file| {
                Self::resolve_bundle_file_entity(bundle.root_dir().to_path_buf(), file)
            })),
            Err(err) => Box::new(std::iter::once(Err(AppleCodesignError::DirectoryBundle(
                err,
            )))),
        }
    }

    fn resolve_bundle_file_entity(
        base_path: PathBuf,
        file: DirectoryBundleFile,
    ) -> Box<dyn Iterator<Item = Result<FileEntity, AppleCodesignError>> + '_> {
        let main_relative_path = match file.absolute_path().strip_prefix(&base_path) {
            Ok(path) => path.to_path_buf(),
            Err(_) => file.absolute_path().to_path_buf(),
        };

        if matches!(file.is_main_executable(), Ok(true)) {
            match std::fs::read(file.absolute_path()) {
                Ok(data) => {
                    let entities =
                        Self::resolve_macho_entities_from_data(&main_relative_path, &data)
                            .collect::<Vec<_>>();

                    Box::new(entities.into_iter())
                }
                Err(err) => Box::new(std::iter::once(Err(err.into()))),
            }
        } else if file.is_code_resources_xml_plist() {
            match std::fs::read(file.absolute_path()) {
                Ok(xml) => Box::new(std::iter::once(Ok(FileEntity {
                    path: main_relative_path,
                    sub_path: None,
                    entity: SignatureEntity::BundleCodeSignatureFile(
                        CodeSignatureFile::ResourcesXml(
                            String::from_utf8_lossy(&xml)
                                .split('\n')
                                .map(|x| x.replace('\t', "  "))
                                .collect::<Vec<_>>(),
                        ),
                    ),
                }))),
                Err(err) => Box::new(std::iter::once(Err(err.into()))),
            }
        } else if file.is_notarization_ticket() {
            match file.metadata() {
                Ok(metadata) => Box::new(std::iter::once(Ok(FileEntity {
                    path: main_relative_path,
                    sub_path: None,
                    entity: SignatureEntity::BundleCodeSignatureFile(
                        CodeSignatureFile::NotarizationTicket(metadata.len()),
                    ),
                }))),
                Err(err) => Box::new(std::iter::once(Err(AppleCodesignError::DirectoryBundle(
                    err,
                )))),
            }
        } else if file.is_in_code_signature_directory() {
            Box::new(std::iter::once(Ok(FileEntity {
                path: main_relative_path,
                sub_path: None,
                entity: SignatureEntity::BundleCodeSignatureFile(CodeSignatureFile::Other),
            })))
        } else {
            Box::new(std::iter::empty())
        }
    }
}
