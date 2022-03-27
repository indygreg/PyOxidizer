// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Common embedded signature data structures (superblobs, magic values, etc).
//!
//! This module defines types and data structures that are common to Apple's
//! embedded signature format.
//!
//! Within this module are constants for header magic, definitions of
//! serialized data structures like superblobs and blobs, and some common
//! enumerations.
//!
//! There is no official specification of the Mach-O structure for various
//! code signing primitives. So the definitions in here could diverge from
//! what is actually implemented.
//!
//! The best source of the specification comes from Apple's open source headers,
//! notably cs_blobs.h (e.g.
//! <https://opensource.apple.com/source/xnu/xnu-7195.81.3/osfmk/kern/cs_blobs.h.auto.html>).
//! (Go to <https://opensource.apple.com/source/xnu> and check for newer versions of xnu
//! to look for new features.)
//!
//! The high-level format of embedded signature data is roughly as follows:
//!
//! * A `SuperBlob` header describes the total length of data and the number of
//!   *blob* sections that follow.
//! * An array of `BlobIndex` describing the type and offset of all *blob* sections
//!   that follow. The *type* here is a *slot* and describes what type of data the
//!   *blob* contains (code directory, entitlements, embedded signature, etc).
//! * N *blob* sections of varying formats and lengths.
//!
//! We only support the [CodeSigningMagic::EmbeddedSignature] magic in the `SuperBlob`,
//! as this is what is used in the wild. (It is even unclear if other magic values
//! can occur in `SuperBlob` headers.)
//!
//! The `EmbeddedSignature` type represents a lightly parsed `SuperBlob`. It
//! provides access to `BlobEntry` which describe the *blob* sections within the
//! super blob. A `BlobEntry` can be parsed into the more concrete `ParsedBlob`,
//! which allows some access to data within each specific blob type.

use {
    crate::{
        code_directory::CodeDirectoryBlob, code_requirement::CodeRequirements,
        code_requirement::RequirementType, AppleCodesignError,
    },
    apple_xar::table_of_contents::ChecksumType as XarChecksumType,
    cryptographic_message_syntax::SignedData,
    scroll::{IOwrite, Pread},
    std::{
        borrow::Cow,
        cmp::Ordering,
        collections::HashMap,
        fmt::{Display, Formatter},
        io::Write,
    },
};

/// Defines header magic for various payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CodeSigningMagic {
    /// Code requirement blob.
    Requirement,
    /// Code requirements blob.
    RequirementSet,
    /// CodeDirectory blob.
    CodeDirectory,
    /// Embedded signature.
    ///
    /// This is often the magic of the SuperBlob.
    EmbeddedSignature,
    /// Old embedded signature.
    EmbeddedSignatureOld,
    /// Entitlements blob.
    Entitlements,
    /// DER encoded entitlements blob.
    EntitlementsDer,
    /// Multi-arch collection of embedded signatures.
    DetachedSignature,
    /// Generic blob wrapper.
    ///
    /// The CMS signature is stored in this type.
    BlobWrapper,
    /// Unknown magic.
    Unknown(u32),
}

impl From<u32> for CodeSigningMagic {
    fn from(v: u32) -> Self {
        match v {
            0xfade0c00 => Self::Requirement,
            0xfade0c01 => Self::RequirementSet,
            0xfade0c02 => Self::CodeDirectory,
            0xfade0cc0 => Self::EmbeddedSignature,
            0xfade0b02 => Self::EmbeddedSignatureOld,
            0xfade7171 => Self::Entitlements,
            0xfade7172 => Self::EntitlementsDer,
            0xfade0cc1 => Self::DetachedSignature,
            0xfade0b01 => Self::BlobWrapper,
            _ => Self::Unknown(v),
        }
    }
}

impl From<CodeSigningMagic> for u32 {
    fn from(magic: CodeSigningMagic) -> u32 {
        match magic {
            CodeSigningMagic::Requirement => 0xfade0c00,
            CodeSigningMagic::RequirementSet => 0xfade0c01,
            CodeSigningMagic::CodeDirectory => 0xfade0c02,
            CodeSigningMagic::EmbeddedSignature => 0xfade0cc0,
            CodeSigningMagic::EmbeddedSignatureOld => 0xfade0b02,
            CodeSigningMagic::Entitlements => 0xfade7171,
            CodeSigningMagic::EntitlementsDer => 0xfade7172,
            CodeSigningMagic::DetachedSignature => 0xfade0cc1,
            CodeSigningMagic::BlobWrapper => 0xfade0b01,
            CodeSigningMagic::Unknown(v) => v,
        }
    }
}

/// A well-known slot within code signing data.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeSigningSlot {
    CodeDirectory,
    Info,
    RequirementSet,
    ResourceDir,
    Application,
    Entitlements,
    RepSpecific,
    EntitlementsDer,
    AlternateCodeDirectory0,
    AlternateCodeDirectory1,
    AlternateCodeDirectory2,
    AlternateCodeDirectory3,
    AlternateCodeDirectory4,
    Signature,
    Identification,
    Ticket,
    Unknown(u32),
}

impl std::fmt::Debug for CodeSigningSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CodeDirectory => {
                f.write_fmt(format_args!("CodeDirectory ({})", u32::from(*self)))
            }
            Self::Info => f.write_fmt(format_args!("Info ({})", u32::from(*self))),
            Self::RequirementSet => {
                f.write_fmt(format_args!("RequirementSet ({})", u32::from(*self)))
            }
            Self::ResourceDir => f.write_fmt(format_args!("Resources ({})", u32::from(*self))),
            Self::Application => f.write_fmt(format_args!("Application ({})", u32::from(*self))),
            Self::Entitlements => f.write_fmt(format_args!("Entitlements ({})", u32::from(*self))),
            Self::RepSpecific => f.write_fmt(format_args!("Rep Specific ({})", u32::from(*self))),
            Self::EntitlementsDer => {
                f.write_fmt(format_args!("DER Entitlements ({})", u32::from(*self)))
            }
            Self::AlternateCodeDirectory0 => f.write_fmt(format_args!(
                "CodeDirectory Alternate #0 ({})",
                u32::from(*self)
            )),
            Self::AlternateCodeDirectory1 => f.write_fmt(format_args!(
                "CodeDirectory Alternate #1 ({})",
                u32::from(*self)
            )),
            Self::AlternateCodeDirectory2 => f.write_fmt(format_args!(
                "CodeDirectory Alternate #2 ({})",
                u32::from(*self)
            )),
            Self::AlternateCodeDirectory3 => f.write_fmt(format_args!(
                "CodeDirectory Alternate #3 ({})",
                u32::from(*self)
            )),
            Self::AlternateCodeDirectory4 => f.write_fmt(format_args!(
                "CodeDirectory Alternate #4 ({})",
                u32::from(*self)
            )),
            Self::Signature => f.write_fmt(format_args!("CMS Signature ({})", u32::from(*self))),
            Self::Identification => {
                f.write_fmt(format_args!("Identification ({})", u32::from(*self)))
            }
            Self::Ticket => f.write_fmt(format_args!("Ticket ({})", u32::from(*self))),
            Self::Unknown(value) => f.write_fmt(format_args!("Unknown ({})", value)),
        }
    }
}

impl From<u32> for CodeSigningSlot {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::CodeDirectory,
            1 => Self::Info,
            2 => Self::RequirementSet,
            3 => Self::ResourceDir,
            4 => Self::Application,
            5 => Self::Entitlements,
            6 => Self::RepSpecific,
            7 => Self::EntitlementsDer,
            0x1000 => Self::AlternateCodeDirectory0,
            0x1001 => Self::AlternateCodeDirectory1,
            0x1002 => Self::AlternateCodeDirectory2,
            0x1003 => Self::AlternateCodeDirectory3,
            0x1004 => Self::AlternateCodeDirectory4,
            0x10000 => Self::Signature,
            0x10001 => Self::Identification,
            0x10002 => Self::Ticket,
            _ => Self::Unknown(v),
        }
    }
}

impl From<CodeSigningSlot> for u32 {
    fn from(v: CodeSigningSlot) -> Self {
        match v {
            CodeSigningSlot::CodeDirectory => 0,
            CodeSigningSlot::Info => 1,
            CodeSigningSlot::RequirementSet => 2,
            CodeSigningSlot::ResourceDir => 3,
            CodeSigningSlot::Application => 4,
            CodeSigningSlot::Entitlements => 5,
            CodeSigningSlot::RepSpecific => 6,
            CodeSigningSlot::EntitlementsDer => 7,
            CodeSigningSlot::AlternateCodeDirectory0 => 0x1000,
            CodeSigningSlot::AlternateCodeDirectory1 => 0x1001,
            CodeSigningSlot::AlternateCodeDirectory2 => 0x1002,
            CodeSigningSlot::AlternateCodeDirectory3 => 0x1003,
            CodeSigningSlot::AlternateCodeDirectory4 => 0x1004,
            CodeSigningSlot::Signature => 0x10000,
            CodeSigningSlot::Identification => 0x10001,
            CodeSigningSlot::Ticket => 0x10002,
            CodeSigningSlot::Unknown(v) => v,
        }
    }
}

impl PartialOrd for CodeSigningSlot {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        u32::from(*self).partial_cmp(&u32::from(*other))
    }
}

impl Ord for CodeSigningSlot {
    fn cmp(&self, other: &Self) -> Ordering {
        u32::from(*self).cmp(&u32::from(*other))
    }
}

impl CodeSigningSlot {
    /// Whether this slot has external data (as opposed to provided via a blob).
    pub fn has_external_content(&self) -> bool {
        matches!(self, Self::Info | Self::ResourceDir)
    }
}

#[repr(C)]
#[derive(Clone, Pread)]
struct BlobIndex {
    /// Corresponds to a [CodeSigningSlot] variant.
    typ: u32,
    offset: u32,
}

impl std::fmt::Debug for BlobIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("BlobIndex")
            .field("type", &CodeSigningSlot::from(self.typ))
            .field("offset", &self.offset)
            .finish()
    }
}

/// Represents a digest type encountered in code signature data structures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DigestType {
    None,
    Sha1,
    Sha256,
    Sha256Truncated,
    Sha384,
    Sha512,
    Unknown(u8),
}

impl Default for DigestType {
    fn default() -> Self {
        Self::Sha256
    }
}

impl From<u8> for DigestType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::None,
            1 => Self::Sha1,
            2 => Self::Sha256,
            3 => Self::Sha256Truncated,
            4 => Self::Sha384,
            5 => Self::Sha512,
            _ => Self::Unknown(v),
        }
    }
}

impl From<DigestType> for u8 {
    fn from(v: DigestType) -> u8 {
        match v {
            DigestType::None => 0,
            DigestType::Sha1 => 1,
            DigestType::Sha256 => 2,
            DigestType::Sha256Truncated => 3,
            DigestType::Sha384 => 4,
            DigestType::Sha512 => 5,
            DigestType::Unknown(v) => v,
        }
    }
}

impl Display for DigestType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DigestType::None => f.write_str("none"),
            DigestType::Sha1 => f.write_str("sha1"),
            DigestType::Sha256 => f.write_str("sha256"),
            DigestType::Sha256Truncated => f.write_str("sha256-truncated"),
            DigestType::Sha384 => f.write_str("sha384"),
            DigestType::Sha512 => f.write_str("sha512"),
            DigestType::Unknown(v) => f.write_fmt(format_args!("unknown: {}", v)),
        }
    }
}

impl TryFrom<&str> for DigestType {
    type Error = AppleCodesignError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "none" => Ok(Self::None),
            "sha1" => Ok(Self::Sha1),
            "sha256" => Ok(Self::Sha256),
            "sha256-truncated" => Ok(Self::Sha256Truncated),
            "sha384" => Ok(Self::Sha384),
            "sha512" => Ok(Self::Sha512),
            _ => Err(AppleCodesignError::DigestUnknownAlgorithm),
        }
    }
}

impl TryFrom<XarChecksumType> for DigestType {
    type Error = AppleCodesignError;

    fn try_from(c: XarChecksumType) -> Result<Self, Self::Error> {
        match c {
            XarChecksumType::None => Ok(Self::None),
            XarChecksumType::Sha1 => Ok(Self::Sha1),
            XarChecksumType::Sha256 => Ok(Self::Sha256),
            XarChecksumType::Sha512 => Ok(Self::Sha512),
            XarChecksumType::Md5 => Err(AppleCodesignError::DigestUnsupportedAlgorithm),
        }
    }
}

impl DigestType {
    /// Obtain the size of hashes for this hash type.
    pub fn hash_len(&self) -> Result<usize, AppleCodesignError> {
        Ok(self.digest_data(&[])?.len())
    }

    /// Obtain a hasher for this digest type.
    pub fn as_hasher(&self) -> Result<ring::digest::Context, AppleCodesignError> {
        match self {
            Self::None => Err(AppleCodesignError::DigestUnknownAlgorithm),
            Self::Sha1 => Ok(ring::digest::Context::new(
                &ring::digest::SHA1_FOR_LEGACY_USE_ONLY,
            )),
            Self::Sha256 | Self::Sha256Truncated => {
                Ok(ring::digest::Context::new(&ring::digest::SHA256))
            }
            Self::Sha384 => Ok(ring::digest::Context::new(&ring::digest::SHA384)),
            Self::Sha512 => Ok(ring::digest::Context::new(&ring::digest::SHA512)),
            Self::Unknown(_) => Err(AppleCodesignError::DigestUnknownAlgorithm),
        }
    }

    /// Digest data given the configured hasher.
    pub fn digest_data(&self, data: &[u8]) -> Result<Vec<u8>, AppleCodesignError> {
        let mut hasher = self.as_hasher()?;

        hasher.update(data);
        let mut hash = hasher.finish().as_ref().to_vec();

        if matches!(self, Self::Sha256Truncated) {
            hash.truncate(20);
        }

        Ok(hash)
    }
}

pub struct Digest<'a> {
    pub data: Cow<'a, [u8]>,
}

impl<'a> Digest<'a> {
    /// Whether this is the null hash (all 0s).
    pub fn is_null(&self) -> bool {
        self.data.iter().all(|b| *b == 0)
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.data.to_vec()
    }

    pub fn to_owned(&self) -> Digest<'static> {
        Digest {
            data: Cow::Owned(self.data.clone().into_owned()),
        }
    }

    pub fn as_hex(&self) -> String {
        hex::encode(&self.data)
    }
}

impl<'a> std::fmt::Debug for Digest<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&hex::encode(&self.data))
    }
}

impl<'a> From<Vec<u8>> for Digest<'a> {
    fn from(v: Vec<u8>) -> Self {
        Self { data: v.into() }
    }
}

/// Read the header from a Blob.
///
/// Blobs begin with a u32 magic and u32 length, inclusive.
fn read_blob_header(data: &[u8]) -> Result<(u32, usize, &[u8]), scroll::Error> {
    let magic = data.pread_with(0, scroll::BE)?;
    let length = data.pread_with::<u32>(4, scroll::BE)?;

    Ok((magic, length as usize, &data[8..]))
}

pub(crate) fn read_and_validate_blob_header<'a>(
    data: &'a [u8],
    expected_magic: u32,
    what: &'static str,
) -> Result<&'a [u8], AppleCodesignError> {
    let (magic, _, data) = read_blob_header(data)?;

    if magic != expected_magic {
        Err(AppleCodesignError::BadMagic(what))
    } else {
        Ok(data)
    }
}

/// Create the binary content for a SuperBlob.
pub fn create_superblob<'a>(
    magic: CodeSigningMagic,
    blobs: impl Iterator<Item = &'a (CodeSigningSlot, Vec<u8>)>,
) -> Result<Vec<u8>, AppleCodesignError> {
    // Makes offset calculation easier.
    let blobs = blobs.collect::<Vec<_>>();

    let mut cursor = std::io::Cursor::new(Vec::<u8>::new());

    let mut blob_data = Vec::new();
    // magic + total length + blob count.
    let mut total_length: u32 = 4 + 4 + 4;
    // 8 bytes for each blob index.
    total_length += 8 * blobs.len() as u32;

    let mut indices = Vec::with_capacity(blobs.len());

    for (slot, blob) in blobs {
        blob_data.push(blob);

        indices.push(BlobIndex {
            typ: u32::from(*slot),
            offset: total_length,
        });

        total_length += blob.len() as u32;
    }

    cursor.iowrite_with(u32::from(magic), scroll::BE)?;
    cursor.iowrite_with(total_length, scroll::BE)?;
    cursor.iowrite_with(indices.len() as u32, scroll::BE)?;
    for index in indices {
        cursor.iowrite_with(index.typ, scroll::BE)?;
        cursor.iowrite_with(index.offset, scroll::BE)?;
    }
    for data in blob_data {
        cursor.write_all(data)?;
    }

    Ok(cursor.into_inner())
}

/// Represents a single blob as defined by a SuperBlob index entry.
///
/// Instances have copies of their own index info, including the relative
/// order, slot type, and start offset within the `SuperBlob`.
///
/// The blob data is unparsed in this type. The blob payloads can be
/// turned into [ParsedBlob] via `.try_into()`.
#[derive(Clone)]
pub struct BlobEntry<'a> {
    /// Our blob index within the `SuperBlob`.
    pub index: usize,

    /// The slot type.
    pub slot: CodeSigningSlot,

    /// Our start offset within the `SuperBlob`.
    ///
    /// First byte is start of our magic.
    pub offset: usize,

    /// The magic value appearing at the beginning of the blob.
    pub magic: CodeSigningMagic,

    /// The length of the blob payload.
    pub length: usize,

    /// The raw data in this blob, including magic and length.
    pub data: &'a [u8],
}

impl<'a> std::fmt::Debug for BlobEntry<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("BlobEntry")
            .field("index", &self.index)
            .field("slot", &self.slot)
            .field("offset", &self.offset)
            .field("length", &self.length)
            .field("magic", &self.magic)
            // .field("data", &self.data)
            .finish()
    }
}

impl<'a> BlobEntry<'a> {
    /// Attempt to convert to a [ParsedBlob].
    pub fn into_parsed_blob(self) -> Result<ParsedBlob<'a>, AppleCodesignError> {
        self.try_into()
    }

    /// Obtain the payload of this blob.
    ///
    /// This is the data in the blob without the blob header.
    pub fn payload(&self) -> Result<&'a [u8], AppleCodesignError> {
        Ok(read_blob_header(self.data)?.2)
    }

    /// Compute the content digest of this blob using the specified hash type.
    pub fn digest_with(&self, hash: DigestType) -> Result<Vec<u8>, AppleCodesignError> {
        hash.digest_data(self.data)
    }
}

/// Provides common features for a parsed blob type.
pub trait Blob<'a>
where
    Self: Sized,
{
    /// The header magic that identifies this format.
    fn magic() -> u32;

    /// Attempt to construct an instance by parsing a bytes slice.
    ///
    /// The slice begins with the 8 byte blob header denoting the magic
    /// and length.
    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError>;

    /// Serialize the payload of this blob to bytes.
    ///
    /// Does not include the magic or length header fields common to blobs.
    fn serialize_payload(&self) -> Result<Vec<u8>, AppleCodesignError>;

    /// Serialize this blob to bytes.
    ///
    /// This is [Blob::serialize_payload] with the blob magic and length
    /// prepended.
    fn to_blob_bytes(&self) -> Result<Vec<u8>, AppleCodesignError> {
        let mut res = Vec::new();
        res.iowrite_with(Self::magic(), scroll::BE)?;

        let payload = self.serialize_payload()?;
        // Length includes our own header.
        res.iowrite_with(payload.len() as u32 + 8, scroll::BE)?;

        res.extend(payload);

        Ok(res)
    }

    /// Obtain the digest of the blob using the specified hasher.
    ///
    /// Default implementation calls [Blob::to_blob_bytes] and digests that, which
    /// should always be correct.
    fn digest_with(&self, hash_type: DigestType) -> Result<Vec<u8>, AppleCodesignError> {
        hash_type.digest_data(&self.to_blob_bytes()?)
    }
}

/// Represents a Requirement blob.
///
/// `csreq -b` will emit instances of this blob, header magic and all. So data generated
/// by `csreq -b` can be fed into [RequirementBlob.from_blob_bytes] to obtain an instance.
pub struct RequirementBlob<'a> {
    pub data: Cow<'a, [u8]>,
}

impl<'a> Blob<'a> for RequirementBlob<'a> {
    fn magic() -> u32 {
        u32::from(CodeSigningMagic::Requirement)
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        let data = read_and_validate_blob_header(data, Self::magic(), "requirement blob")?;

        Ok(Self { data: data.into() })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, AppleCodesignError> {
        Ok(self.data.to_vec())
    }
}

impl<'a> std::fmt::Debug for RequirementBlob<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("RequirementBlob({})", hex::encode(&self.data)))
    }
}

impl<'a> RequirementBlob<'a> {
    pub fn to_owned(&self) -> RequirementBlob<'static> {
        RequirementBlob {
            data: Cow::Owned(self.data.clone().into_owned()),
        }
    }

    /// Parse the binary data in this blob into Code Requirement expressions.
    pub fn parse_expressions(&self) -> Result<CodeRequirements, AppleCodesignError> {
        Ok(CodeRequirements::parse_binary(&self.data)?.0)
    }
}

/// Represents a Requirement set blob.
///
/// A Requirement set blob contains nested Requirement blobs.
#[derive(Debug, Default)]
pub struct RequirementSetBlob<'a> {
    pub requirements: HashMap<RequirementType, RequirementBlob<'a>>,
}

impl<'a> Blob<'a> for RequirementSetBlob<'a> {
    fn magic() -> u32 {
        u32::from(CodeSigningMagic::RequirementSet)
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        read_and_validate_blob_header(data, Self::magic(), "requirement set blob")?;

        // There are other blobs nested within. A u32 denotes how many there are.
        // Then there is an array of N (u32, u32) denoting the type and
        // offset of each.
        let offset = &mut 8;
        let count = data.gread_with::<u32>(offset, scroll::BE)?;

        let mut indices = Vec::with_capacity(count as usize);
        for _ in 0..count {
            indices.push((
                data.gread_with::<u32>(offset, scroll::BE)?,
                data.gread_with::<u32>(offset, scroll::BE)?,
            ));
        }

        let mut requirements = HashMap::with_capacity(indices.len());

        for (i, (flavor, offset)) in indices.iter().enumerate() {
            let typ = RequirementType::from(*flavor);

            let end_offset = if i == indices.len() - 1 {
                data.len()
            } else {
                indices[i + 1].1 as usize
            };

            let requirement_data = &data[*offset as usize..end_offset];

            requirements.insert(typ, RequirementBlob::from_blob_bytes(requirement_data)?);
        }

        Ok(Self { requirements })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, AppleCodesignError> {
        let mut res = Vec::new();

        // The index contains blob relative offsets. To know what the start offset will
        // be, we calculate the total index size.
        let data_start_offset = 8 + 4 + (8 * self.requirements.len() as u32);
        let mut written_requirements_data = 0;

        res.iowrite_with(self.requirements.len() as u32, scroll::BE)?;

        // Write an index of all nested requirement blobs.
        for (typ, requirement) in &self.requirements {
            res.iowrite_with(u32::from(*typ), scroll::BE)?;
            res.iowrite_with(data_start_offset + written_requirements_data, scroll::BE)?;
            written_requirements_data += requirement.to_blob_bytes()?.len() as u32;
        }

        // Now write every requirement's raw data.
        for requirement in self.requirements.values() {
            res.write_all(&requirement.to_blob_bytes()?)?;
        }

        Ok(res)
    }
}

impl<'a> RequirementSetBlob<'a> {
    pub fn to_owned(&self) -> RequirementSetBlob<'static> {
        RequirementSetBlob {
            requirements: self
                .requirements
                .iter()
                .map(|(flavor, blob)| (*flavor, blob.to_owned()))
                .collect::<HashMap<_, _>>(),
        }
    }

    /// Set the requirements for a given [RequirementType].
    pub fn set_requirements(&mut self, slot: RequirementType, blob: RequirementBlob<'a>) {
        self.requirements.insert(slot, blob);
    }
}

/// Represents an embedded signature.
#[derive(Debug)]
pub struct EmbeddedSignatureBlob<'a> {
    data: &'a [u8],
}

impl<'a> Blob<'a> for EmbeddedSignatureBlob<'a> {
    fn magic() -> u32 {
        u32::from(CodeSigningMagic::EmbeddedSignature)
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        Ok(Self {
            data: read_and_validate_blob_header(data, Self::magic(), "embedded signature blob")?,
        })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, AppleCodesignError> {
        Ok(self.data.to_vec())
    }
}

/// An old embedded signature.
#[derive(Debug)]
pub struct EmbeddedSignatureOldBlob<'a> {
    data: &'a [u8],
}

impl<'a> Blob<'a> for EmbeddedSignatureOldBlob<'a> {
    fn magic() -> u32 {
        u32::from(CodeSigningMagic::EmbeddedSignatureOld)
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        Ok(Self {
            data: read_and_validate_blob_header(
                data,
                Self::magic(),
                "old embedded signature blob",
            )?,
        })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, AppleCodesignError> {
        Ok(self.data.to_vec())
    }
}

/// Represents an Entitlements blob.
///
/// An entitlements blob contains an XML plist with a dict. Keys are
/// strings of the entitlements being requested and values appear to be
/// simple bools.
#[derive(Debug)]
pub struct EntitlementsBlob<'a> {
    plist: Cow<'a, str>,
}

impl<'a> Blob<'a> for EntitlementsBlob<'a> {
    fn magic() -> u32 {
        u32::from(CodeSigningMagic::Entitlements)
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        let data = read_and_validate_blob_header(data, Self::magic(), "entitlements blob")?;
        let s = std::str::from_utf8(data).map_err(AppleCodesignError::EntitlementsBadUtf8)?;

        Ok(Self { plist: s.into() })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, AppleCodesignError> {
        Ok(self.plist.as_bytes().to_vec())
    }
}

impl<'a> EntitlementsBlob<'a> {
    /// Construct an instance using any string as the payload.
    pub fn from_string(s: &(impl ToString + ?Sized)) -> Self {
        Self {
            plist: s.to_string().into(),
        }
    }

    /// Obtain the plist representation as a string.
    pub fn as_str(&self) -> &str {
        &self.plist
    }
}

impl<'a> std::fmt::Display for EntitlementsBlob<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.plist)
    }
}

#[derive(Debug)]
pub struct EntitlementsDerBlob<'a> {
    der: Cow<'a, [u8]>,
}

impl<'a> Blob<'a> for EntitlementsDerBlob<'a> {
    fn magic() -> u32 {
        u32::from(CodeSigningMagic::EntitlementsDer)
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        let der = read_and_validate_blob_header(data, Self::magic(), "DER entitlements blob")?;

        Ok(Self { der: der.into() })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, AppleCodesignError> {
        Ok(self.der.to_vec())
    }
}

impl<'a> EntitlementsDerBlob<'a> {
    /// Construct an instance from a [plist::Value].
    ///
    /// Not all plists can be encoded to this blob as not all plist value types can
    /// be encoded to DER. If a plist with an illegal value is passed in, this
    /// function will error, as DER encoding is performed immediately.
    ///
    /// The outermost plist value should be a dictionary.
    pub fn from_plist(v: &plist::Value) -> Result<Self, AppleCodesignError> {
        let der = crate::entitlements::der_encode_entitlements_plist(v)?;

        Ok(Self { der: der.into() })
    }
}

/// A detached signature.
#[derive(Debug)]
pub struct DetachedSignatureBlob<'a> {
    data: &'a [u8],
}

impl<'a> Blob<'a> for DetachedSignatureBlob<'a> {
    fn magic() -> u32 {
        u32::from(CodeSigningMagic::DetachedSignature)
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        Ok(Self {
            data: read_and_validate_blob_header(data, Self::magic(), "detached signature blob")?,
        })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, AppleCodesignError> {
        Ok(self.data.to_vec())
    }
}

/// Represents a generic blob wrapper.
pub struct BlobWrapperBlob<'a> {
    data: Cow<'a, [u8]>,
}

impl<'a> Blob<'a> for BlobWrapperBlob<'a> {
    fn magic() -> u32 {
        u32::from(CodeSigningMagic::BlobWrapper)
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        Ok(Self {
            data: read_and_validate_blob_header(data, Self::magic(), "blob wrapper blob")?.into(),
        })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, AppleCodesignError> {
        Ok(self.data.to_vec())
    }
}

impl<'a> std::fmt::Debug for BlobWrapperBlob<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", hex::encode(&self.data)))
    }
}

impl<'a> BlobWrapperBlob<'a> {
    /// Construct an instance where the payload (post blob header) is given data.
    pub fn from_data_borrowed(data: &'a [u8]) -> BlobWrapperBlob<'a> {
        Self { data: data.into() }
    }
}

impl<'a> BlobWrapperBlob<'static> {
    /// Construct an instance with payload data.
    pub fn from_data_owned(data: Vec<u8>) -> BlobWrapperBlob<'static> {
        Self { data: data.into() }
    }
}

/// Represents an unknown blob type.
pub struct OtherBlob<'a> {
    pub magic: u32,
    pub data: &'a [u8],
}

impl<'a> Blob<'a> for OtherBlob<'a> {
    fn magic() -> u32 {
        // Use a placeholder magic value because there is no self bind here.
        u32::MAX
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        let (magic, _, data) = read_blob_header(data)?;

        Ok(Self { magic, data })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, AppleCodesignError> {
        Ok(self.data.to_vec())
    }

    // We need to implement this for custom magic serialization.
    fn to_blob_bytes(&self) -> Result<Vec<u8>, AppleCodesignError> {
        let mut res = Vec::with_capacity(self.data.len() + 8);
        res.iowrite_with(self.magic, scroll::BE)?;
        res.iowrite_with(self.data.len() as u32 + 8, scroll::BE)?;
        res.write_all(self.data)?;

        Ok(res)
    }
}

impl<'a> std::fmt::Debug for OtherBlob<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", hex::encode(self.data)))
    }
}

/// Represents a single, parsed Blob entry/slot.
///
/// Each variant corresponds to a [CodeSigningMagic] blob type.
#[derive(Debug)]
pub enum BlobData<'a> {
    Requirement(Box<RequirementBlob<'a>>),
    RequirementSet(Box<RequirementSetBlob<'a>>),
    CodeDirectory(Box<CodeDirectoryBlob<'a>>),
    EmbeddedSignature(Box<EmbeddedSignatureBlob<'a>>),
    EmbeddedSignatureOld(Box<EmbeddedSignatureOldBlob<'a>>),
    Entitlements(Box<EntitlementsBlob<'a>>),
    EntitlementsDer(Box<EntitlementsDerBlob<'a>>),
    DetachedSignature(Box<DetachedSignatureBlob<'a>>),
    BlobWrapper(Box<BlobWrapperBlob<'a>>),
    Other(Box<OtherBlob<'a>>),
}

impl<'a> Blob<'a> for BlobData<'a> {
    fn magic() -> u32 {
        u32::MAX
    }

    /// Parse blob data by reading its magic and feeding into magic-specific parser.
    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        let (magic, length, _) = read_blob_header(data)?;

        // This should be a no-op. But it could (correctly) cause a panic if the
        // advertised length is incorrect and we would incur a buffer overrun.
        let data = &data[0..length];

        let magic = CodeSigningMagic::from(magic);

        Ok(match magic {
            CodeSigningMagic::Requirement => {
                Self::Requirement(Box::new(RequirementBlob::from_blob_bytes(data)?))
            }
            CodeSigningMagic::RequirementSet => {
                Self::RequirementSet(Box::new(RequirementSetBlob::from_blob_bytes(data)?))
            }
            CodeSigningMagic::CodeDirectory => {
                Self::CodeDirectory(Box::new(CodeDirectoryBlob::from_blob_bytes(data)?))
            }
            CodeSigningMagic::EmbeddedSignature => {
                Self::EmbeddedSignature(Box::new(EmbeddedSignatureBlob::from_blob_bytes(data)?))
            }
            CodeSigningMagic::EmbeddedSignatureOld => Self::EmbeddedSignatureOld(Box::new(
                EmbeddedSignatureOldBlob::from_blob_bytes(data)?,
            )),
            CodeSigningMagic::Entitlements => {
                Self::Entitlements(Box::new(EntitlementsBlob::from_blob_bytes(data)?))
            }
            CodeSigningMagic::EntitlementsDer => {
                Self::EntitlementsDer(Box::new(EntitlementsDerBlob::from_blob_bytes(data)?))
            }
            CodeSigningMagic::DetachedSignature => {
                Self::DetachedSignature(Box::new(DetachedSignatureBlob::from_blob_bytes(data)?))
            }
            CodeSigningMagic::BlobWrapper => {
                Self::BlobWrapper(Box::new(BlobWrapperBlob::from_blob_bytes(data)?))
            }
            _ => Self::Other(Box::new(OtherBlob::from_blob_bytes(data)?)),
        })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, AppleCodesignError> {
        match self {
            Self::Requirement(b) => b.serialize_payload(),
            Self::RequirementSet(b) => b.serialize_payload(),
            Self::CodeDirectory(b) => b.serialize_payload(),
            Self::EmbeddedSignature(b) => b.serialize_payload(),
            Self::EmbeddedSignatureOld(b) => b.serialize_payload(),
            Self::Entitlements(b) => b.serialize_payload(),
            Self::EntitlementsDer(b) => b.serialize_payload(),
            Self::DetachedSignature(b) => b.serialize_payload(),
            Self::BlobWrapper(b) => b.serialize_payload(),
            Self::Other(b) => b.serialize_payload(),
        }
    }

    fn to_blob_bytes(&self) -> Result<Vec<u8>, AppleCodesignError> {
        match self {
            Self::Requirement(b) => b.to_blob_bytes(),
            Self::RequirementSet(b) => b.to_blob_bytes(),
            Self::CodeDirectory(b) => b.to_blob_bytes(),
            Self::EmbeddedSignature(b) => b.to_blob_bytes(),
            Self::EmbeddedSignatureOld(b) => b.to_blob_bytes(),
            Self::Entitlements(b) => b.to_blob_bytes(),
            Self::EntitlementsDer(b) => b.to_blob_bytes(),
            Self::DetachedSignature(b) => b.to_blob_bytes(),
            Self::BlobWrapper(b) => b.to_blob_bytes(),
            Self::Other(b) => b.to_blob_bytes(),
        }
    }
}

impl<'a> From<RequirementBlob<'a>> for BlobData<'a> {
    fn from(b: RequirementBlob<'a>) -> Self {
        Self::Requirement(Box::new(b))
    }
}

impl<'a> From<RequirementSetBlob<'a>> for BlobData<'a> {
    fn from(b: RequirementSetBlob<'a>) -> Self {
        Self::RequirementSet(Box::new(b))
    }
}

impl<'a> From<CodeDirectoryBlob<'a>> for BlobData<'a> {
    fn from(b: CodeDirectoryBlob<'a>) -> Self {
        Self::CodeDirectory(Box::new(b))
    }
}

impl<'a> From<EmbeddedSignatureBlob<'a>> for BlobData<'a> {
    fn from(b: EmbeddedSignatureBlob<'a>) -> Self {
        Self::EmbeddedSignature(Box::new(b))
    }
}

impl<'a> From<EmbeddedSignatureOldBlob<'a>> for BlobData<'a> {
    fn from(b: EmbeddedSignatureOldBlob<'a>) -> Self {
        Self::EmbeddedSignatureOld(Box::new(b))
    }
}

impl<'a> From<EntitlementsBlob<'a>> for BlobData<'a> {
    fn from(b: EntitlementsBlob<'a>) -> Self {
        Self::Entitlements(Box::new(b))
    }
}

impl<'a> From<EntitlementsDerBlob<'a>> for BlobData<'a> {
    fn from(b: EntitlementsDerBlob<'a>) -> Self {
        Self::EntitlementsDer(Box::new(b))
    }
}

impl<'a> From<DetachedSignatureBlob<'a>> for BlobData<'a> {
    fn from(b: DetachedSignatureBlob<'a>) -> Self {
        Self::DetachedSignature(Box::new(b))
    }
}

impl<'a> From<BlobWrapperBlob<'a>> for BlobData<'a> {
    fn from(b: BlobWrapperBlob<'a>) -> Self {
        Self::BlobWrapper(Box::new(b))
    }
}

impl<'a> From<OtherBlob<'a>> for BlobData<'a> {
    fn from(b: OtherBlob<'a>) -> Self {
        Self::Other(Box::new(b))
    }
}

/// Represents the parsed content of a blob entry.
#[derive(Debug)]
pub struct ParsedBlob<'a> {
    /// The blob record this blob came from.
    pub blob_entry: BlobEntry<'a>,

    /// The parsed blob data.
    pub blob: BlobData<'a>,
}

impl<'a> ParsedBlob<'a> {
    /// Compute the content digest of this blob using the specified hash type.
    pub fn digest_with(&self, hash: DigestType) -> Result<Vec<u8>, AppleCodesignError> {
        hash.digest_data(self.blob_entry.data)
    }
}

impl<'a> TryFrom<BlobEntry<'a>> for ParsedBlob<'a> {
    type Error = AppleCodesignError;

    fn try_from(blob_entry: BlobEntry<'a>) -> Result<Self, Self::Error> {
        let blob = BlobData::from_blob_bytes(blob_entry.data)?;

        Ok(Self { blob_entry, blob })
    }
}

/// Represents Apple's common embedded code signature data structures.
///
/// This type represents a lightly parsed `SuperBlob` with [CodeSigningMagic::EmbeddedSignature].
/// It is the most common embedded signature data format you are likely to encounter.
pub struct EmbeddedSignature<'a> {
    /// Magic value from header.
    pub magic: CodeSigningMagic,
    /// Length of this super blob.
    pub length: u32,
    /// Number of blobs in this super blob.
    pub count: u32,

    /// Raw data backing this super blob.
    pub data: &'a [u8],

    /// All the blobs within this super blob.
    pub blobs: Vec<BlobEntry<'a>>,
}

impl<'a> std::fmt::Debug for EmbeddedSignature<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("SuperBlob")
            .field("magic", &self.magic)
            .field("length", &self.length)
            .field("count", &self.count)
            .field("blobs", &self.blobs)
            .finish()
    }
}

// There are other impl blocks for this structure in other modules.
impl<'a> EmbeddedSignature<'a> {
    /// Attempt to parse an embedded signature super blob from data.
    ///
    /// The argument to this function is likely the subset of the
    /// `__LINKEDIT` Mach-O section that the `LC_CODE_SIGNATURE` load instructions
    /// points it.
    pub fn from_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        let offset = &mut 0;

        // Parse the 3 fields from the SuperBlob.
        let magic = data.gread_with::<u32>(offset, scroll::BE)?.into();

        if magic != CodeSigningMagic::EmbeddedSignature {
            return Err(AppleCodesignError::BadMagic(
                "embedded signature super blob",
            ));
        }

        let length = data.gread_with(offset, scroll::BE)?;
        let count = data.gread_with(offset, scroll::BE)?;

        // Following the SuperBlob header is an array of .count BlobIndex defining
        // the Blob that follow.
        //
        // The BlobIndex doesn't declare the length of each Blob. However, it appears
        // the first 8 bytes of each blob contain the u32 magic and u32 length.
        // We do parse those here and set the blob length/slice accordingly. However,
        // we take an extra level of precaution by first computing a slice that doesn't
        // overrun into the next blob or past the end of the input buffer. This
        // helps detect invalid length advertisements in the blob payload.
        let mut blob_indices = Vec::with_capacity(count as usize);
        for _ in 0..count {
            blob_indices.push(data.gread_with::<BlobIndex>(offset, scroll::BE)?);
        }

        let mut blobs = Vec::with_capacity(blob_indices.len());

        for (i, index) in blob_indices.iter().enumerate() {
            let end_offset = if i == blob_indices.len() - 1 {
                data.len()
            } else {
                blob_indices[i + 1].offset as usize
            };

            let full_slice = &data[index.offset as usize..end_offset];
            let (magic, blob_length, _) = read_blob_header(full_slice)?;

            // Self-reported length can't be greater than the data we have.
            let blob_data = match blob_length.cmp(&full_slice.len()) {
                Ordering::Greater => {
                    return Err(AppleCodesignError::SuperblobMalformed);
                }
                Ordering::Equal => full_slice,
                Ordering::Less => &full_slice[0..blob_length],
            };

            blobs.push(BlobEntry {
                index: i,
                slot: index.typ.into(),
                offset: index.offset as usize,
                magic: magic.into(),
                length: blob_length,
                data: blob_data,
            });
        }

        Ok(Self {
            magic,
            length,
            count,
            data,
            blobs,
        })
    }

    /// Find the first occurrence of the specified slot.
    pub fn find_slot(&self, slot: CodeSigningSlot) -> Option<&BlobEntry<'a>> {
        self.blobs.iter().find(|e| e.slot == slot)
    }

    pub fn find_slot_parsed(
        &self,
        slot: CodeSigningSlot,
    ) -> Result<Option<ParsedBlob<'a>>, AppleCodesignError> {
        if let Some(entry) = self.find_slot(slot) {
            Ok(Some(entry.clone().into_parsed_blob()?))
        } else {
            Ok(None)
        }
    }

    /// Attempt to resolve a parsed `CodeDirectoryBlob` for this signature data.
    ///
    /// Returns Err on data parsing error or if the blob slot didn't contain a code
    /// directory.
    ///
    /// Returns `Ok(None)` if there is no code directory slot.
    pub fn code_directory(&self) -> Result<Option<Box<CodeDirectoryBlob<'a>>>, AppleCodesignError> {
        if let Some(parsed) = self.find_slot_parsed(CodeSigningSlot::CodeDirectory)? {
            if let BlobData::CodeDirectory(cd) = parsed.blob {
                Ok(Some(cd))
            } else {
                Err(AppleCodesignError::BadMagic("code directory blob"))
            }
        } else {
            Ok(None)
        }
    }

    /// Attempt to resolve a parsed [EntitlementsBlob] for this signature data.
    ///
    /// Returns Err on data parsing error or if the blob slot didn't contain an entitlments
    /// blob.
    ///
    /// Returns `Ok(None)` if there is no entitlements slot.
    pub fn entitlements(&self) -> Result<Option<Box<EntitlementsBlob<'a>>>, AppleCodesignError> {
        if let Some(parsed) = self.find_slot_parsed(CodeSigningSlot::Entitlements)? {
            if let BlobData::Entitlements(entitlements) = parsed.blob {
                Ok(Some(entitlements))
            } else {
                Err(AppleCodesignError::BadMagic("entitlements blob"))
            }
        } else {
            Ok(None)
        }
    }

    /// Attempt to resolve a parsed [RequirementSetBlob] for this signature data.
    ///
    /// Returns Err on data parsing error or if the blob slot didn't contain a requirements
    /// blob.
    ///
    /// Returns `Ok(None)` if there is no requirements slot.
    pub fn code_requirements(
        &self,
    ) -> Result<Option<Box<RequirementSetBlob<'a>>>, AppleCodesignError> {
        if let Some(parsed) = self.find_slot_parsed(CodeSigningSlot::RequirementSet)? {
            if let BlobData::RequirementSet(reqs) = parsed.blob {
                Ok(Some(reqs))
            } else {
                Err(AppleCodesignError::BadMagic("requirements blob"))
            }
        } else {
            Ok(None)
        }
    }

    /// Attempt to resolve raw CMS signature data.
    ///
    /// The returned data is likely DER PKCS#7 with the root object
    /// pkcs7-signedData (1.2.840.113549.1.7.2).
    pub fn signature_data(&self) -> Result<Option<&'a [u8]>, AppleCodesignError> {
        if let Some(parsed) = self.find_slot(CodeSigningSlot::Signature) {
            // Make sure it validates.
            ParsedBlob::try_from(parsed.clone())?;

            Ok(Some(parsed.payload()?))
        } else {
            Ok(None)
        }
    }

    /// Obtain the parsed CMS [SignedData].
    pub fn signed_data(&self) -> Result<Option<SignedData>, AppleCodesignError> {
        if let Some(data) = self.signature_data()? {
            // Sometime we get an empty data slice. This has been observed on DMG signatures.
            // In that scenario, pretend there is no CMS data at all.
            if data.is_empty() {
                Ok(None)
            } else {
                let signed_data = SignedData::parse_ber(data)?;

                Ok(Some(signed_data))
            }
        } else {
            Ok(None)
        }
    }
}
