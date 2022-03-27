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
    apple_xar::{
        reader::XarReader,
        table_of_contents::{File as XarTocFile, Signature as XarTocSignature},
    },
    cryptographic_message_syntax::{SignedData, SignerInfo},
    goblin::mach::{fat::FAT_MAGIC, parse_magic_and_ctx, Mach, MachO},
    serde::Serialize,
    std::{
        fmt::Debug,
        fs::File,
        io::{BufWriter, Cursor, Read, Seek},
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

#[allow(unused)]
fn pretty_print_xml(xml: &[u8]) -> Result<Vec<u8>, AppleCodesignError> {
    let mut reader = xml::reader::EventReader::new(Cursor::new(xml));
    let mut emitter = xml::EmitterConfig::new()
        .perform_indent(true)
        .create_writer(BufWriter::new(Vec::with_capacity(xml.len() * 2)));

    while let Ok(event) = reader.next() {
        if matches!(event, xml::reader::XmlEvent::EndDocument) {
            break;
        }

        if let Some(event) = event.as_writer_event() {
            emitter.write(event).map_err(AppleCodesignError::XmlWrite)?;
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
    NotarizationTicket,
    Other,
}

#[derive(Clone, Debug, Serialize)]
pub struct XarTableOfContents {
    pub toc_length_compressed: u64,
    pub toc_length_uncompressed: u64,
    pub checksum_type: String,
    pub toc_start_offset: u16,
    pub heap_start_offset: u64,
    pub creation_time: String,
    pub toc_checksum: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<XarSignature>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_signature: Option<XarSignature>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub xml: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rsa_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cms_signature: Option<CmsSignature>,
}

impl XarTableOfContents {
    pub fn from_xar<R: Read + Seek + Sized + Debug>(
        xar: &mut XarReader<R>,
    ) -> Result<Self, AppleCodesignError> {
        let (digest_type, digest) = xar.checksum()?;
        let _xml = xar.table_of_contents_decoded_data()?;

        let rsa_signature = xar.rsa_signature()?.map(|x| hex::encode(x.0));
        let cms_signature = if let Some(signed_data) = xar.cms_signature()? {
            Some(CmsSignature::try_from(signed_data)?)
        } else {
            None
        };

        let header = xar.header();
        let toc = xar.table_of_contents();

        // This can be useful for debugging.
        //String::from_utf8_lossy(&pretty_print_xml(&xml)?)
        //    .lines()
        //    .map(|x| x.to_string())
        //    .collect::<Vec<_>>();
        let xml = vec![];

        Ok(Self {
            toc_length_compressed: header.toc_length_compressed,
            toc_length_uncompressed: header.toc_length_uncompressed,
            checksum_type: apple_xar::format::XarChecksum::from(header.checksum_algorithm_id)
                .to_string(),
            toc_start_offset: header.size,
            heap_start_offset: xar.heap_start_offset(),
            creation_time: toc.creation_time.clone(),
            toc_checksum: format!("{}:{}", digest_type, hex::encode(digest)),
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
            cms_signature,
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
    pub data_offset: Option<u64>,
    pub data_size: Option<u64>,
    pub data_length: Option<u64>,
    pub data_end_offset: Option<u64>,
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
        self.data_offset = Some(data.offset);
        self.data_size = Some(data.size);
        self.data_length = Some(data.length);
        self.data_end_offset = Some(data.offset + data.length);
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
    pub file_size: u64,
    pub file_sha256: String,
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

        Ok(Self {
            path: report_path,
            file_size: metadata.len(),
            file_sha256: hex::encode(DigestAlgorithm::Sha256.digest_path(path)?),
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
                Mach::parse(&data)?;

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

        if file
            .is_main_executable()
            .map_err(AppleCodesignError::DirectoryBundle)?
        {
            let data = std::fs::read(file.absolute_path())?;
            entities.extend(Self::resolve_macho_entities_from_data(
                file.absolute_path(),
                &data,
                Some(&main_relative_path),
            )?);
        } else if file.is_code_resources_xml_plist() {
            let data = std::fs::read(file.absolute_path())?;

            default_entity.entity =
                SignatureEntity::BundleCodeSignatureFile(CodeSignatureFile::ResourcesXml(
                    String::from_utf8_lossy(&data)
                        .split('\n')
                        .map(|x| x.replace('\t', "  "))
                        .collect::<Vec<_>>(),
                ));

            entities.push(default_entity);
        } else if file.is_notarization_ticket() {
            default_entity.entity =
                SignatureEntity::BundleCodeSignatureFile(CodeSignatureFile::NotarizationTicket);

            entities.push(default_entity);
        } else if file.is_in_code_signature_directory() {
            default_entity.entity =
                SignatureEntity::BundleCodeSignatureFile(CodeSignatureFile::Other);

            entities.push(default_entity);
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
