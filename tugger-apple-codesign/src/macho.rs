// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Mach-O primitives related to code signing

There is no official specification of the Mach-O structure for various
code signing primitives. So the definitions in here could diverge from
what is actually implemented.

The best source of the specification comes from Apple's open source headers,
notably cs_blobs.h (e.g.
https://opensource.apple.com/source/xnu/xnu-7195.81.3/osfmk/kern/cs_blobs.h.auto.html).
(Go to https://opensource.apple.com/source/xnu and check for newer versions of xnu
to look for new features.)

Code signing data is embedded within the named `__LINKEDIT` segment of
the Mach-O binary. An `LC_CODE_SIGNATURE` load command in the Mach-O header
will point you at this data. See `find_signature_data()` for this logic.

Within the `__LINKEDIT` segment we have a number of data structures
describing code signing information. The high-level format of these
data structures within the segment is roughly as follows:

* A `SuperBlob` header describes the total length of data and the number of
  *blob* sections that follow.
* An array of `BlobIndex` describing the type and offset of all *blob* sections
  that follow. The *type* here is a *slot* and describes what type of data the
  *blob* contains (code directory, entitlements, embedded signature, etc).
* N *blob* sections of varying formats and lengths.

We only support the `CSMAGIC_EMBEDDED_SIGNATURE` magic in the `SuperBlob`, as
this is what is used in the wild. (It is even unclear if other `CSMAGIC_*`
values can occur in `SuperBlob` headers.)

The `EmbeddedSignature` type represents a lightly parsed `SuperBlob`. It
provides access to `BlobEntry` which describe the *blob* sections within the
super blob. A `BlobEntry` can be parsed into the more concrete `ParsedBlob`,
which allows some access to data within each specific blob type.
*/

use {
    goblin::mach::{constants::SEG_LINKEDIT, load_command::CommandVariant, MachO},
    scroll::{IOwrite, Pread},
    std::{
        borrow::Cow,
        cmp::Ordering,
        collections::HashMap,
        convert::{TryFrom, TryInto},
        io::Write,
    },
};

// Constants identifying payload of Blob entries.
const CSSLOT_CODEDIRECTORY: u32 = 0;
const CSSLOT_INFOSLOT: u32 = 1;
const CSSLOT_REQUIREMENTS: u32 = 2;
const CSSLOT_RESOURCEDIR: u32 = 3;
const CSSLOT_APPLICATION: u32 = 4;
const CSSLOT_ENTITLEMENTS: u32 = 5;

// This slot appears to contain a binary plist with executable security settings.
// We don't know what to call it.
const CSSLOT_SECURITYSETTINGS: u32 = 7;

/// First alternate CodeDirectory, if any
const CSSLOT_ALTERNATE_CODEDIRECTORY_0: u32 = 0x1000;
const CSSLOT_ALTERNATE_CODEDIRECTORY_1: u32 = 0x1001;
const CSSLOT_ALTERNATE_CODEDIRECTORY_2: u32 = 0x1002;
const CSSLOT_ALTERNATE_CODEDIRECTORY_3: u32 = 0x1003;
const CSSLOT_ALTERNATE_CODEDIRECTORY_4: u32 = 0x1004;

/// CMS signature.
const CSSLOT_SIGNATURESLOT: u32 = 0x10000;
const CSSLOT_IDENTIFICATIONSLOT: u32 = 0x10001;
const CSSLOT_TICKETSLOT: u32 = 0x10002;

/// Defines a typed slot within code signing data.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeSigningSlot {
    CodeDirectory,
    Info,
    Requirements,
    ResourceDir,
    Application,
    Entitlements,
    SecuritySettings,
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
            Self::Requirements => f.write_fmt(format_args!("Requirements ({})", u32::from(*self))),
            Self::ResourceDir => f.write_fmt(format_args!("Resources ({})", u32::from(*self))),
            Self::Application => f.write_fmt(format_args!("Application ({})", u32::from(*self))),
            Self::Entitlements => f.write_fmt(format_args!("Entitlements ({})", u32::from(*self))),
            Self::SecuritySettings => {
                f.write_fmt(format_args!("SecuritySettings ({})", u32::from(*self)))
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
            CSSLOT_CODEDIRECTORY => Self::CodeDirectory,
            CSSLOT_INFOSLOT => Self::Info,
            CSSLOT_REQUIREMENTS => Self::Requirements,
            CSSLOT_RESOURCEDIR => Self::ResourceDir,
            CSSLOT_APPLICATION => Self::Application,
            CSSLOT_ENTITLEMENTS => Self::Entitlements,
            CSSLOT_SECURITYSETTINGS => Self::SecuritySettings,
            CSSLOT_ALTERNATE_CODEDIRECTORY_0 => Self::AlternateCodeDirectory0,
            CSSLOT_ALTERNATE_CODEDIRECTORY_1 => Self::AlternateCodeDirectory1,
            CSSLOT_ALTERNATE_CODEDIRECTORY_2 => Self::AlternateCodeDirectory2,
            CSSLOT_ALTERNATE_CODEDIRECTORY_3 => Self::AlternateCodeDirectory3,
            CSSLOT_ALTERNATE_CODEDIRECTORY_4 => Self::AlternateCodeDirectory4,
            CSSLOT_SIGNATURESLOT => Self::Signature,
            CSSLOT_IDENTIFICATIONSLOT => Self::Identification,
            CSSLOT_TICKETSLOT => Self::Ticket,
            _ => Self::Unknown(v),
        }
    }
}

impl From<CodeSigningSlot> for u32 {
    fn from(v: CodeSigningSlot) -> Self {
        match v {
            CodeSigningSlot::CodeDirectory => CSSLOT_CODEDIRECTORY,
            CodeSigningSlot::Info => CSSLOT_INFOSLOT,
            CodeSigningSlot::Requirements => CSSLOT_REQUIREMENTS,
            CodeSigningSlot::ResourceDir => CSSLOT_RESOURCEDIR,
            CodeSigningSlot::Application => CSSLOT_APPLICATION,
            CodeSigningSlot::Entitlements => CSSLOT_ENTITLEMENTS,
            CodeSigningSlot::SecuritySettings => CSSLOT_SECURITYSETTINGS,
            CodeSigningSlot::AlternateCodeDirectory0 => CSSLOT_ALTERNATE_CODEDIRECTORY_0,
            CodeSigningSlot::AlternateCodeDirectory1 => CSSLOT_ALTERNATE_CODEDIRECTORY_1,
            CodeSigningSlot::AlternateCodeDirectory2 => CSSLOT_ALTERNATE_CODEDIRECTORY_2,
            CodeSigningSlot::AlternateCodeDirectory3 => CSSLOT_ALTERNATE_CODEDIRECTORY_3,
            CodeSigningSlot::AlternateCodeDirectory4 => CSSLOT_ALTERNATE_CODEDIRECTORY_4,
            CodeSigningSlot::Signature => CSSLOT_SIGNATURESLOT,
            CodeSigningSlot::Identification => CSSLOT_IDENTIFICATIONSLOT,
            CodeSigningSlot::Ticket => CSSLOT_TICKETSLOT,
            CodeSigningSlot::Unknown(v) => v,
        }
    }
}

/// Single requirement blob.
const CSMAGIC_REQUIREMENT: u32 = 0xfade0c00;

/// Requirements vector (internal requirements).
const CSMAGIC_REQUIREMENTS: u32 = 0xfade0c01;

/// CodeDirectory blob.
const CSMAGIC_CODEDIRECTORY: u32 = 0xfade0c02;

/// Embedded form of signature data.
const CSMAGIC_EMBEDDED_SIGNATURE: u32 = 0xfade0cc0;

/// XXX
const CSMAGIC_EMBEDDED_SIGNATURE_OLD: u32 = 0xfade0b02;

/// Embedded entitlements.
const CSMAGIC_EMBEDDED_ENTITLEMENTS: u32 = 0xfade7171;

/// Security settings.
const CSMAGIC_SECURITY_SETTINGS: u32 = 0xfade7172;

/// Multi-arch collection of embedded signatures.
const CSMAGIC_DETACHED_SIGNATURE: u32 = 0xfade0cc1;

/// CMS signature, among other things.
const CSMAGIC_BLOBWRAPPER: u32 = 0xfade0b01;

/// Defines header magic for various payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CodeSigningMagic {
    Requirement,
    Requirements,
    CodeDirectory,
    EmbeddedSignature,
    EmbeddedSignatureOld,
    EmbeddedEntitlements,
    SecuritySettings,
    DetachedSignature,
    BlobWrapper,
    Unknown(u32),
}

impl From<u32> for CodeSigningMagic {
    fn from(v: u32) -> Self {
        match v {
            CSMAGIC_REQUIREMENT => Self::Requirement,
            CSMAGIC_REQUIREMENTS => Self::Requirements,
            CSMAGIC_CODEDIRECTORY => Self::CodeDirectory,
            CSMAGIC_EMBEDDED_SIGNATURE => Self::EmbeddedSignature,
            CSMAGIC_EMBEDDED_SIGNATURE_OLD => Self::EmbeddedSignatureOld,
            CSMAGIC_EMBEDDED_ENTITLEMENTS => Self::EmbeddedEntitlements,
            CSMAGIC_SECURITY_SETTINGS => Self::SecuritySettings,
            CSMAGIC_DETACHED_SIGNATURE => Self::DetachedSignature,
            CSMAGIC_BLOBWRAPPER => Self::BlobWrapper,
            _ => Self::Unknown(v),
        }
    }
}

impl From<CodeSigningMagic> for u32 {
    fn from(magic: CodeSigningMagic) -> u32 {
        match magic {
            CodeSigningMagic::Requirement => CSMAGIC_REQUIREMENT,
            CodeSigningMagic::Requirements => CSMAGIC_REQUIREMENTS,
            CodeSigningMagic::CodeDirectory => CSMAGIC_CODEDIRECTORY,
            CodeSigningMagic::EmbeddedSignature => CSMAGIC_EMBEDDED_SIGNATURE,
            CodeSigningMagic::EmbeddedSignatureOld => CSMAGIC_EMBEDDED_SIGNATURE_OLD,
            CodeSigningMagic::EmbeddedEntitlements => CSMAGIC_EMBEDDED_ENTITLEMENTS,
            CodeSigningMagic::SecuritySettings => CSMAGIC_SECURITY_SETTINGS,
            CodeSigningMagic::DetachedSignature => CSMAGIC_DETACHED_SIGNATURE,
            CodeSigningMagic::BlobWrapper => CSMAGIC_BLOBWRAPPER,
            CodeSigningMagic::Unknown(v) => v,
        }
    }
}

// Executable segment flags.

/// Executable segment denotes main binary.
pub const CS_EXECSEG_MAIN_BINARY: u32 = 0x1;

/// Allow unsigned pages (for debugging)
pub const CS_EXECSEG_ALLOW_UNSIGNED: u32 = 0x10;

/// Main binary is debugger.
pub const CS_EXECSEG_DEBUGGER: u32 = 0x20;

/// JIT enabled.
pub const CS_EXECSEG_JIT: u32 = 0x40;

/// Obsolete: skip library validation.
pub const CS_EXECSEG_SKIP_LV: u32 = 0x80;

/// Can bless cdhash for execution.
pub const CS_EXECSEG_CAN_LOAD_CDHASH: u32 = 0x100;

/// Can execute blessed cdhash.
pub const CS_EXECSEG_CAN_EXEC_CDHASH: u32 = 0x200;

// Versions of CodeDirectory struct.
const CS_SUPPORTSSCATTER: u32 = 0x20100;
const CS_SUPPORTSTEAMID: u32 = 0x20200;
const CS_SUPPORTSCODELIMIT64: u32 = 0x20300;
const CS_SUPPORTSEXECSEG: u32 = 0x20400;
const CS_SUPPORTSRUNTIME: u32 = 0x20500;
const CS_SUPPORTSLINKAGE: u32 = 0x20600;

/// Compat with amfi
pub const CSTYPE_INDEX_REQUIREMENTS: u32 = 0x00000002;
pub const CSTYPE_INDEX_ENTITLEMENTS: u32 = 0x00000005;

const CS_HASHTYPE_SHA1: u8 = 1;
const CS_HASHTYPE_SHA256: u8 = 2;
const CS_HASHTYPE_SHA256_TRUNCATED: u8 = 3;
const CS_HASHTYPE_SHA384: u8 = 4;

pub const CS_SHA1_LEN: u32 = 20;
pub const CS_SHA256_LEN: u32 = 32;
pub const CS_SHA256_TRUNCATED_LEN: u32 = 20;

/// always - larger hashes are truncated
pub const CS_CDHASH_LEN: u32 = 20;
/// max size of the hash we'll support
pub const CS_HASH_MAX_SIZE: u32 = 48;

/*
 * Currently only to support Legacy VPN plugins, and Mac App Store
 * but intended to replace all the various platform code, dev code etc. bits.
 */
pub const CS_SIGNER_TYPE_UNKNOWN: u32 = 0;
pub const CS_SIGNER_TYPE_LEGACYVPN: u32 = 5;
pub const CS_SIGNER_TYPE_MAC_APP_STORE: u32 = 6;

pub const CS_SUPPL_SIGNER_TYPE_UNKNOWN: u32 = 0;
pub const CS_SUPPL_SIGNER_TYPE_TRUSTCACHE: u32 = 7;
pub const CS_SUPPL_SIGNER_TYPE_LOCAL: u32 = 8;

#[repr(C)]
#[derive(Clone, Pread)]
struct BlobIndex {
    /// Corresponds to a CSSLOT_* constant.
    typ: u32,
    offset: u32,
}

/// Read the header from a Blob.
///
/// Blobs begin with a u32 magic and u32 length, inclusive.
fn read_blob_header(data: &[u8]) -> Result<(u32, usize, &[u8]), scroll::Error> {
    let magic = data.pread_with(0, scroll::BE)?;
    let length = data.pread_with::<u32>(4, scroll::BE)?;

    Ok((magic, length as usize, &data[8..]))
}

fn read_and_validate_blob_header(data: &[u8], expected_magic: u32) -> Result<&[u8], MachOError> {
    let (magic, _, data) = read_blob_header(data)?;

    if magic != expected_magic {
        Err(MachOError::BadMagic)
    } else {
        Ok(data)
    }
}

impl std::fmt::Debug for BlobIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("BlobIndex")
            .field("type", &CodeSigningSlot::from(self.typ))
            .field("offset", &self.offset)
            .finish()
    }
}

/// Create the binary content for a SuperBlob.
pub fn create_superblob<'a>(
    magic: CodeSigningMagic,
    blobs: impl Iterator<Item = &'a (CodeSigningSlot, Vec<u8>)>,
) -> Result<Vec<u8>, MachOError> {
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

/// Represents embedded signature data in a Mach-O binary.
///
/// This type represents a lightly parsed `SuperBlob` with
/// `CSMAGIC_EMBEDDED_SIGNATURE` magic embedded in a Mach-O binary. It is the
/// most common embedded signature data format you are likely to encounter.
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
    pub fn from_bytes(data: &'a [u8]) -> Result<Self, MachOError> {
        let offset = &mut 0;

        // Parse the 3 fields from the SuperBlob.
        let magic = data.gread_with::<u32>(offset, scroll::BE)?.into();

        if magic != CodeSigningMagic::EmbeddedSignature {
            return Err(MachOError::BadMagic);
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
                    return Err(MachOError::Malformed);
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
    pub fn find_slot(&self, slot: CodeSigningSlot) -> Option<&BlobEntry> {
        self.blobs.iter().find(|e| e.slot == slot)
    }

    pub fn find_slot_parsed(
        &self,
        slot: CodeSigningSlot,
    ) -> Result<Option<ParsedBlob<'_>>, MachOError> {
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
    pub fn code_directory(&self) -> Result<Option<Box<CodeDirectoryBlob<'_>>>, MachOError> {
        if let Some(parsed) = self.find_slot_parsed(CodeSigningSlot::CodeDirectory)? {
            if let BlobData::CodeDirectory(cd) = parsed.blob {
                Ok(Some(cd))
            } else {
                Err(MachOError::BadMagic)
            }
        } else {
            Ok(None)
        }
    }

    /// Attempt to resolve a parsed `EntitlementsBlob` for this signature data.
    ///
    /// Returns Err on data parsing error or if the blob slot didn't contain an entitlments
    /// blob.
    ///
    /// Returns `Ok(None)` if there is no entitlements slot.
    pub fn entitlements(&self) -> Result<Option<Box<EntitlementsBlob<'_>>>, MachOError> {
        if let Some(parsed) = self.find_slot_parsed(CodeSigningSlot::Entitlements)? {
            if let BlobData::EmbeddedEntitlements(entitlements) = parsed.blob {
                Ok(Some(entitlements))
            } else {
                Err(MachOError::BadMagic)
            }
        } else {
            Ok(None)
        }
    }

    /// Attempt to resolve a parsed `RequirementsBlob` for this signature data.
    ///
    /// Returns Err on data parsing error or if the blob slot didn't contain a requirements
    /// blob.
    ///
    /// Returns `Ok(None)` if there is no requirements slot.
    pub fn code_requirements(&self) -> Result<Option<Box<RequirementsBlob<'_>>>, MachOError> {
        if let Some(parsed) = self.find_slot_parsed(CodeSigningSlot::Requirements)? {
            if let BlobData::Requirements(reqs) = parsed.blob {
                Ok(Some(reqs))
            } else {
                Err(MachOError::BadMagic)
            }
        } else {
            Ok(None)
        }
    }

    /// Attempt to resolve raw signature data from `SignatureBlob`.
    ///
    /// The returned data is likely DER PKCS#7 with the root object
    /// pkcs7-signedData (1.2.840.113549.1.7.2).
    pub fn signature_data(&self) -> Result<Option<&'_ [u8]>, MachOError> {
        if let Some(parsed) = self.find_slot_parsed(CodeSigningSlot::Signature)? {
            if let BlobData::BlobWrapper(blob) = parsed.blob {
                Ok(Some(blob.data))
            } else {
                Err(MachOError::BadMagic)
            }
        } else {
            Ok(None)
        }
    }
}

/// Represents a single blob as defined by a `SuperBlob` index entry.
///
/// Instances have copies of their own index info, including the relative
/// order, slot type, and start offset within the `SuperBlob`.
///
/// The blob data is unparsed in this type. The blob payloads can be
/// turned into `ParsedBlob` via `.try_into()`.
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
    /// Attempt to convert to a `ParsedBlob`.
    pub fn into_parsed_blob(self) -> Result<ParsedBlob<'a>, MachOError> {
        self.try_into()
    }

    /// Compute the content digest of this blob using the specified hash type.
    pub fn digest_with(&self, hash: DigestType) -> Result<Vec<u8>, DigestError> {
        hash.digest(&self.data)
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
    pub fn digest_with(&self, hash: DigestType) -> Result<Vec<u8>, DigestError> {
        hash.digest(&self.blob_entry.data)
    }
}

impl<'a> TryFrom<BlobEntry<'a>> for ParsedBlob<'a> {
    type Error = MachOError;

    fn try_from(blob_entry: BlobEntry<'a>) -> Result<Self, Self::Error> {
        let blob = BlobData::from_blob_bytes(blob_entry.data)?;

        Ok(Self { blob_entry, blob })
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
    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, MachOError>;

    /// Serialize the payload of this blob to bytes.
    ///
    /// Does not include the magic or length header fields common to blobs.
    fn serialize_payload(&self) -> Result<Vec<u8>, MachOError>;

    /// Serialize this blob to bytes.
    ///
    /// This is `serialize_payload()` with the blob magic and length
    /// prepended.
    fn to_blob_bytes(&self) -> Result<Vec<u8>, MachOError> {
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
    fn digest_with(&self, hash_type: DigestType) -> Result<Vec<u8>, MachOError> {
        Ok(hash_type.digest(&self.to_blob_bytes()?)?)
    }
}

/// Represents a single, parsed Blob entry/slot.
///
/// Each variant corresponds to a CSMAGIC_* blob type.
#[derive(Debug)]
pub enum BlobData<'a> {
    Requirement(Box<RequirementBlob<'a>>),
    Requirements(Box<RequirementsBlob<'a>>),
    CodeDirectory(Box<CodeDirectoryBlob<'a>>),
    EmbeddedSignature(Box<EmbeddedSignatureBlob<'a>>),
    EmbeddedSignatureOld(Box<EmbeddedSignatureOldBlob<'a>>),
    EmbeddedEntitlements(Box<EntitlementsBlob<'a>>),
    DetachedSignature(Box<DetachedSignatureBlob<'a>>),
    BlobWrapper(Box<BlobWrapperBlob<'a>>),
    Other(Box<OtherBlob<'a>>),
}

impl<'a> Blob<'a> for BlobData<'a> {
    fn magic() -> u32 {
        u32::MAX
    }

    /// Parse blob data by reading its magic and feeding into magic-specific parser.
    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, MachOError> {
        let (magic, length, _) = read_blob_header(data)?;

        // This should be a no-op. But it could (correctly) cause a panic if the
        // advertised length is incorrect and we would incur a buffer overrun.
        let data = &data[0..length];

        Ok(match magic {
            CSMAGIC_REQUIREMENT => {
                Self::Requirement(Box::new(RequirementBlob::from_blob_bytes(data)?))
            }
            CSMAGIC_REQUIREMENTS => {
                Self::Requirements(Box::new(RequirementsBlob::from_blob_bytes(data)?))
            }
            CSMAGIC_CODEDIRECTORY => {
                Self::CodeDirectory(Box::new(CodeDirectoryBlob::from_blob_bytes(data)?))
            }
            CSMAGIC_EMBEDDED_SIGNATURE => {
                Self::EmbeddedSignature(Box::new(EmbeddedSignatureBlob::from_blob_bytes(data)?))
            }
            CSMAGIC_EMBEDDED_SIGNATURE_OLD => Self::EmbeddedSignatureOld(Box::new(
                EmbeddedSignatureOldBlob::from_blob_bytes(data)?,
            )),
            CSMAGIC_EMBEDDED_ENTITLEMENTS => {
                Self::EmbeddedEntitlements(Box::new(EntitlementsBlob::from_blob_bytes(data)?))
            }
            CSMAGIC_DETACHED_SIGNATURE => {
                Self::DetachedSignature(Box::new(DetachedSignatureBlob::from_blob_bytes(data)?))
            }
            CSMAGIC_BLOBWRAPPER => {
                Self::BlobWrapper(Box::new(BlobWrapperBlob::from_blob_bytes(data)?))
            }
            _ => Self::Other(Box::new(OtherBlob::from_blob_bytes(data)?)),
        })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, MachOError> {
        match self {
            Self::Requirement(b) => b.serialize_payload(),
            Self::Requirements(b) => b.serialize_payload(),
            Self::CodeDirectory(b) => b.serialize_payload(),
            Self::EmbeddedSignature(b) => b.serialize_payload(),
            Self::EmbeddedSignatureOld(b) => b.serialize_payload(),
            Self::EmbeddedEntitlements(b) => b.serialize_payload(),
            Self::DetachedSignature(b) => b.serialize_payload(),
            Self::BlobWrapper(b) => b.serialize_payload(),
            Self::Other(b) => b.serialize_payload(),
        }
    }

    fn to_blob_bytes(&self) -> Result<Vec<u8>, MachOError> {
        match self {
            Self::Requirement(b) => b.to_blob_bytes(),
            Self::Requirements(b) => b.to_blob_bytes(),
            Self::CodeDirectory(b) => b.to_blob_bytes(),
            Self::EmbeddedSignature(b) => b.to_blob_bytes(),
            Self::EmbeddedSignatureOld(b) => b.to_blob_bytes(),
            Self::EmbeddedEntitlements(b) => b.to_blob_bytes(),
            Self::DetachedSignature(b) => b.to_blob_bytes(),
            Self::BlobWrapper(b) => b.to_blob_bytes(),
            Self::Other(b) => b.to_blob_bytes(),
        }
    }
}

/// Represents a Requirement blob (CSMAGIC_REQUIREMENT).
///
/// It appears `csreq -b` will emit instances of this blob, header magic and
/// all. So data generated by `csreq -b` can be fed into [RequirementBlob.from_blob_bytes]
/// to obtain an instance.
pub struct RequirementBlob<'a> {
    pub data: Cow<'a, [u8]>,
}

impl<'a> Blob<'a> for RequirementBlob<'a> {
    fn magic() -> u32 {
        CSMAGIC_REQUIREMENT
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, MachOError> {
        let data = read_and_validate_blob_header(data, Self::magic())?;

        Ok(Self { data: data.into() })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, MachOError> {
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
}

/// Represents a Requirements blob (CSMAGIC_REQUIREMENTS).
///
/// A Requirements blob contains nested Requirement blobs.
#[derive(Debug)]
pub struct RequirementsBlob<'a> {
    pub segments: Vec<(u32, RequirementBlob<'a>)>,
}

impl<'a> Blob<'a> for RequirementsBlob<'a> {
    fn magic() -> u32 {
        CSMAGIC_REQUIREMENTS
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, MachOError> {
        read_and_validate_blob_header(data, Self::magic())?;

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

        let mut segments = Vec::with_capacity(indices.len());

        // TODO figure out what the first integer means. It means something.
        for (i, (flavor, offset)) in indices.iter().enumerate() {
            let end_offset = if i == indices.len() - 1 {
                data.len()
            } else {
                indices[i + 1].1 as usize
            };

            let segment_data = &data[*offset as usize..end_offset];

            segments.push((*flavor, RequirementBlob::from_blob_bytes(segment_data)?));
        }

        Ok(Self { segments })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, MachOError> {
        let mut res = Vec::new();

        // The index contains blob relative offsets. To know what the start offset will
        // be, we calculate the total index size.
        let data_start_offset = 8 + 4 + (8 * self.segments.len() as u32);
        let mut written_requirements_data = 0;

        res.iowrite_with(self.segments.len() as u32, scroll::BE)?;

        // Write an index of all nested requirement blobs.
        for (flavor, requirement) in &self.segments {
            res.iowrite_with(*flavor, scroll::BE)?;
            res.iowrite_with(data_start_offset + written_requirements_data, scroll::BE)?;
            written_requirements_data += requirement.to_blob_bytes()?.len() as u32;
        }

        // Now write every requirement's raw data.
        for (_, requirement) in &self.segments {
            res.write_all(&requirement.to_blob_bytes()?)?;
        }

        Ok(res)
    }
}

impl<'a> RequirementsBlob<'a> {
    pub fn to_owned(&self) -> RequirementsBlob<'static> {
        RequirementsBlob {
            segments: self
                .segments
                .iter()
                .map(|(flavor, blob)| (*flavor, blob.to_owned()))
                .collect::<Vec<_>>(),
        }
    }
}

#[derive(Debug)]
pub enum DigestError {
    UnknownAlgorithm,
    UnsupportedAlgorithm,
    Unspecified,
}

impl std::error::Error for DigestError {}

impl std::fmt::Display for DigestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownAlgorithm => f.write_str("unknown algorithm"),
            Self::UnsupportedAlgorithm => f.write_str("unsupported algorithm"),
            Self::Unspecified => f.write_str("unspecified error occurred"),
        }
    }
}

/// Represents a digest type from a CS_HASHTYPE_* constants.
#[derive(Clone, Copy, Debug)]
pub enum DigestType {
    None,
    Sha1,
    Sha256,
    Sha256Truncated,
    Sha384,
    Unknown(u8),
}

impl From<u8> for DigestType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::None,
            CS_HASHTYPE_SHA1 => Self::Sha1,
            CS_HASHTYPE_SHA256 => Self::Sha256,
            CS_HASHTYPE_SHA256_TRUNCATED => Self::Sha256Truncated,
            CS_HASHTYPE_SHA384 => Self::Sha384,
            _ => Self::Unknown(v),
        }
    }
}

impl From<DigestType> for u8 {
    fn from(v: DigestType) -> u8 {
        match v {
            DigestType::None => 0,
            DigestType::Sha1 => CS_HASHTYPE_SHA1,
            DigestType::Sha256 => CS_HASHTYPE_SHA256,
            DigestType::Sha256Truncated => CS_HASHTYPE_SHA256_TRUNCATED,
            DigestType::Sha384 => CS_HASHTYPE_SHA384,
            DigestType::Unknown(v) => v,
        }
    }
}

impl TryFrom<&str> for DigestType {
    type Error = DigestError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "none" => Ok(Self::None),
            "sha1" => Ok(Self::Sha1),
            "sha256" => Ok(Self::Sha256),
            "sha256-truncated" => Ok(Self::Sha256Truncated),
            "sha384" => Ok(Self::Sha384),
            _ => Err(DigestError::UnknownAlgorithm),
        }
    }
}

impl DigestType {
    /// Obtain the size of hashes for this hash type.
    pub fn hash_len(&self) -> Result<usize, DigestError> {
        Ok(self.digest(&[])?.len())
    }

    /// Obtain a hasher for this digest type.
    pub fn as_hasher(&self) -> Result<ring::digest::Context, DigestError> {
        match self {
            Self::Sha1 => Ok(ring::digest::Context::new(
                &ring::digest::SHA1_FOR_LEGACY_USE_ONLY,
            )),
            Self::Sha256 | Self::Sha256Truncated => {
                Ok(ring::digest::Context::new(&ring::digest::SHA256))
            }
            Self::Sha384 => Ok(ring::digest::Context::new(&ring::digest::SHA384)),
            _ => Err(DigestError::UnknownAlgorithm),
        }
    }

    /// Digest data given the configured hasher.
    pub fn digest(&self, data: &[u8]) -> Result<Vec<u8>, DigestError> {
        let mut hasher = self.as_hasher()?;

        hasher.update(data);
        let hash = hasher.finish().as_ref().to_vec();

        // TODO truncate hash.
        if matches!(self, Self::Sha256Truncated) {
            return Err(DigestError::UnsupportedAlgorithm);
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
}

impl<'a> std::fmt::Debug for Digest<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&hex::encode(&self.data))
    }
}

fn get_hashes(data: &[u8], offset: usize, count: usize, hash_size: usize) -> Vec<Digest<'_>> {
    data[offset..offset + (count * hash_size)]
        .chunks(hash_size)
        .map(|data| Digest { data: data.into() })
        .collect()
}

/// Represents a code directory blob entry (CSSLOT_CODEDIRECTORY).
///
/// This struct is versioned and has been extended over time.
///
/// The struct here represents a superset of all fields in all versions.
///
/// The parser will set `Option<T>` fields to `None` for instances
/// where the version is lower than the version that field was introduced in.
#[derive(Debug)]
pub struct CodeDirectoryBlob<'a> {
    /// Compatibility version.
    pub version: u32,
    /// Setup and mode flags.
    pub flags: u32,
    // hash_offset, ident_offset, n_special_slots, and n_code_slots not stored
    // explicitly because they are redundant with derived fields.
    /// Limit to main image signature range.
    ///
    /// This is the file-level offset to stop digesting code data at.
    /// It likely corresponds to the file-offset offset where the
    /// embedded signature data starts in the `__LINKEDIT` segment.
    pub code_limit: u32,
    /// Size of each hash in bytes.
    pub hash_size: u8,
    /// Type of hash.
    pub hash_type: DigestType,
    /// Platform identifier. 0 if not platform binary.
    pub platform: u8,
    /// Page size in bytes. (stored as log u8)
    pub page_size: u32,
    /// Unused (must be 0).
    pub spare2: u32,
    // Version 0x20100
    /// Offset of optional scatter vector.
    pub scatter_offset: Option<u32>,
    // Version 0x20200
    // team_offset not stored because it is redundant with derived stored str.
    // Version 0x20300
    /// Unused (must be 0).
    pub spare3: Option<u32>,
    /// Limit to main image signature range, 64 bits.
    pub code_limit_64: Option<u64>,
    // Version 0x20400
    /// Offset of executable segment.
    pub exec_seg_base: Option<u64>,
    /// Limit of executable segment.
    pub exec_seg_limit: Option<u64>,
    /// Executable segment flags.
    pub exec_seg_flags: Option<u64>,
    // Version 0x20500
    pub runtime: Option<u32>,
    pub pre_encrypt_offset: Option<u32>,
    // Version 0x20600
    pub linkage_hash_type: Option<u8>,
    pub linkage_truncated: Option<u8>,
    pub spare4: Option<u16>,
    pub linkage_offset: Option<u32>,
    pub linkage_size: Option<u32>,

    // End of blob header data / start of derived data.
    pub ident: Cow<'a, str>,
    pub team_name: Option<Cow<'a, str>>,
    pub code_hashes: Vec<Digest<'a>>,
    pub special_hashes: HashMap<CodeSigningSlot, Digest<'a>>,
}

impl<'a> Blob<'a> for CodeDirectoryBlob<'a> {
    fn magic() -> u32 {
        CSMAGIC_CODEDIRECTORY
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, MachOError> {
        read_and_validate_blob_header(data, Self::magic())?;

        let offset = &mut 8;

        let version = data.gread_with(offset, scroll::BE)?;
        let flags = data.gread_with(offset, scroll::BE)?;
        assert_eq!(*offset, 0x10);
        let hash_offset = data.gread_with::<u32>(offset, scroll::BE)?;
        let ident_offset = data.gread_with::<u32>(offset, scroll::BE)?;
        let n_special_slots = data.gread_with::<u32>(offset, scroll::BE)?;
        let n_code_slots = data.gread_with::<u32>(offset, scroll::BE)?;
        assert_eq!(*offset, 0x20);
        let code_limit = data.gread_with(offset, scroll::BE)?;
        let hash_size = data.gread_with(offset, scroll::BE)?;
        let hash_type = data.gread_with::<u8>(offset, scroll::BE)?.into();
        let platform = data.gread_with(offset, scroll::BE)?;
        let page_size = data.gread_with::<u8>(offset, scroll::BE)?;
        let page_size = 2u32.pow(page_size as u32);
        let spare2 = data.gread_with(offset, scroll::BE)?;

        let scatter_offset = if version >= CS_SUPPORTSSCATTER {
            let v = data.gread_with(offset, scroll::BE)?;

            if v != 0 {
                Some(v)
            } else {
                None
            }
        } else {
            None
        };
        let team_offset = if version >= CS_SUPPORTSTEAMID {
            assert_eq!(*offset, 0x30);
            let v = data.gread_with::<u32>(offset, scroll::BE)?;

            if v != 0 {
                Some(v)
            } else {
                None
            }
        } else {
            None
        };

        let (spare3, code_limit_64) = if version >= CS_SUPPORTSCODELIMIT64 {
            (
                Some(data.gread_with(offset, scroll::BE)?),
                Some(data.gread_with(offset, scroll::BE)?),
            )
        } else {
            (None, None)
        };

        let (exec_seg_base, exec_seg_limit, exec_seg_flags) = if version >= CS_SUPPORTSEXECSEG {
            assert_eq!(*offset, 0x40);
            (
                Some(data.gread_with(offset, scroll::BE)?),
                Some(data.gread_with(offset, scroll::BE)?),
                Some(data.gread_with(offset, scroll::BE)?),
            )
        } else {
            (None, None, None)
        };

        let (runtime, pre_encrypt_offset) = if version >= CS_SUPPORTSRUNTIME {
            assert_eq!(*offset, 0x58);
            (
                Some(data.gread_with(offset, scroll::BE)?),
                Some(data.gread_with(offset, scroll::BE)?),
            )
        } else {
            (None, None)
        };

        let (linkage_hash_type, linkage_truncated, spare4, linkage_offset, linkage_size) =
            if version >= CS_SUPPORTSLINKAGE {
                assert_eq!(*offset, 0x60);
                (
                    Some(data.gread_with(offset, scroll::BE)?),
                    Some(data.gread_with(offset, scroll::BE)?),
                    Some(data.gread_with(offset, scroll::BE)?),
                    Some(data.gread_with(offset, scroll::BE)?),
                    Some(data.gread_with(offset, scroll::BE)?),
                )
            } else {
                (None, None, None, None, None)
            };

        // Find trailing null in identifier string.
        let ident = match data[ident_offset as usize..]
            .split(|&b| b == 0)
            .map(std::str::from_utf8)
            .next()
        {
            Some(res) => Cow::from(res?),
            None => {
                return Err(MachOError::BadIdentifierString);
            }
        };

        let team_name = if let Some(team_offset) = team_offset {
            match data[team_offset as usize..]
                .split(|&b| b == 0)
                .map(std::str::from_utf8)
                .next()
            {
                Some(res) => Some(Cow::from(res?)),
                None => {
                    return Err(MachOError::BadTeamString);
                }
            }
        } else {
            None
        };

        let code_hashes = get_hashes(
            data,
            hash_offset as usize,
            n_code_slots as usize,
            hash_size as usize,
        );

        let special_hashes = get_hashes(
            data,
            (hash_offset - (hash_size as u32 * n_special_slots)) as usize,
            n_special_slots as usize,
            hash_size as usize,
        )
        .into_iter()
        .enumerate()
        .map(|(i, h)| (CodeSigningSlot::from(n_special_slots - i as u32), h))
        .collect();

        Ok(Self {
            version,
            flags,
            code_limit,
            hash_size,
            hash_type,
            platform,
            page_size,
            spare2,
            scatter_offset,
            spare3,
            code_limit_64,
            exec_seg_base,
            exec_seg_limit,
            exec_seg_flags,
            runtime,
            pre_encrypt_offset,
            linkage_hash_type,
            linkage_truncated,
            spare4,
            linkage_offset,
            linkage_size,
            ident,
            team_name,
            code_hashes,
            special_hashes,
        })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, MachOError> {
        let mut cursor = std::io::Cursor::new(Vec::<u8>::new());

        // We need to do this in 2 phases because we don't know the length until
        // we build up the data structure.

        cursor.iowrite_with(self.version, scroll::BE)?;
        cursor.iowrite_with(self.flags, scroll::BE)?;
        let hash_offset_cursor_position = cursor.position();
        cursor.iowrite_with(0u32, scroll::BE)?;
        let ident_offset_cursor_position = cursor.position();
        cursor.iowrite_with(0u32, scroll::BE)?;
        assert_eq!(cursor.position(), 0x10);

        // Hash offsets and counts are wonky. The recorded hash offset is the beginning
        // of code hashes and special hashes are in "negative" indices before
        // that offset. Hashes are also at the index of their CSSLOT_ constant.
        // e.g. Code Directory is the first element in the specials array because
        // it is slot 0. This means we need to write out empty hashes for missing
        // special slots. Our local specials HashMap may not have all entries. So compute
        // how many specials there should be and write that here. We'll insert placeholder
        // digests later.
        let highest_slot = self
            .special_hashes
            .keys()
            .map(|slot| u32::from(*slot))
            .max()
            .unwrap_or(0);

        cursor.iowrite_with(highest_slot as u32, scroll::BE)?;
        cursor.iowrite_with(self.code_hashes.len() as u32, scroll::BE)?;
        cursor.iowrite_with(self.code_limit, scroll::BE)?;
        cursor.iowrite_with(self.hash_size, scroll::BE)?;
        cursor.iowrite_with(u8::from(self.hash_type), scroll::BE)?;
        cursor.iowrite_with(self.platform, scroll::BE)?;
        cursor.iowrite_with(self.page_size.trailing_zeros() as u8, scroll::BE)?;
        assert_eq!(cursor.position(), 0x20);
        cursor.iowrite_with(self.spare2, scroll::BE)?;

        let mut scatter_offset_cursor_position = None;
        let mut team_offset_cursor_position = None;

        if self.version >= CS_SUPPORTSSCATTER {
            scatter_offset_cursor_position = Some(cursor.position());
            cursor.iowrite_with(self.scatter_offset.unwrap_or(0), scroll::BE)?;

            if self.version >= CS_SUPPORTSTEAMID {
                team_offset_cursor_position = Some(cursor.position());
                cursor.iowrite_with(0u32, scroll::BE)?;

                if self.version >= CS_SUPPORTSCODELIMIT64 {
                    cursor.iowrite_with(self.spare3.unwrap_or(0), scroll::BE)?;
                    assert_eq!(cursor.position(), 0x30);
                    cursor.iowrite_with(self.code_limit_64.unwrap_or(0), scroll::BE)?;

                    if self.version >= CS_SUPPORTSEXECSEG {
                        cursor.iowrite_with(self.exec_seg_base.unwrap_or(0), scroll::BE)?;
                        assert_eq!(cursor.position(), 0x40);
                        cursor.iowrite_with(self.exec_seg_limit.unwrap_or(0), scroll::BE)?;
                        cursor.iowrite_with(self.exec_seg_flags.unwrap_or(0), scroll::BE)?;

                        if self.version >= CS_SUPPORTSRUNTIME {
                            assert_eq!(cursor.position(), 0x50);
                            cursor.iowrite_with(self.runtime.unwrap_or(0), scroll::BE)?;
                            cursor
                                .iowrite_with(self.pre_encrypt_offset.unwrap_or(0), scroll::BE)?;

                            if self.version >= CS_SUPPORTSLINKAGE {
                                cursor.iowrite_with(
                                    self.linkage_hash_type.unwrap_or(0),
                                    scroll::BE,
                                )?;
                                cursor.iowrite_with(
                                    self.linkage_truncated.unwrap_or(0),
                                    scroll::BE,
                                )?;
                                cursor.iowrite_with(self.spare4.unwrap_or(0), scroll::BE)?;
                                cursor
                                    .iowrite_with(self.linkage_offset.unwrap_or(0), scroll::BE)?;
                                assert_eq!(cursor.position(), 0x60);
                                cursor.iowrite_with(self.linkage_size.unwrap_or(0), scroll::BE)?;
                            }
                        }
                    }
                }
            }
        }

        // We've written all the struct fields. Now write variable length fields.

        let identity_offset = cursor.position();
        cursor.write_all(self.ident.as_bytes())?;
        cursor.write_all(b"\0")?;

        let team_offset = cursor.position();
        if team_offset_cursor_position.is_some() {
            if let Some(team_name) = &self.team_name {
                cursor.write_all(team_name.as_bytes())?;
                cursor.write_all(b"\0")?;
            }
        }

        // TODO consider aligning cursor on page boundary here for performance?

        // The boundary conditions are a bit wonky here. We want to go from greatest
        // to smallest, not writing index 0 because that's the first code digest.
        for slot_index in (1..highest_slot + 1).rev() {
            if let Some(hash) = self.special_hashes.get(&CodeSigningSlot::from(slot_index)) {
                cursor.write_all(&hash.data)?;
            } else {
                cursor.write_all(&b"\0".repeat(self.hash_size as usize))?;
            }
        }

        let code_hashes_start_offset = cursor.position();

        for hash in &self.code_hashes {
            cursor.write_all(&hash.data)?;
        }

        // TODO write out scatter vector.

        // Now go back and update the placeholder offsets. We need to add 8 to account
        // for the blob header, which isn't present in this buffer.
        cursor.set_position(hash_offset_cursor_position);
        cursor.iowrite_with(code_hashes_start_offset as u32 + 8, scroll::BE)?;

        cursor.set_position(ident_offset_cursor_position);
        cursor.iowrite_with(identity_offset as u32 + 8, scroll::BE)?;

        if scatter_offset_cursor_position.is_some() && self.scatter_offset.is_some() {
            return Err(MachOError::Unimplemented);
        }

        if let Some(offset) = team_offset_cursor_position {
            if self.team_name.is_some() {
                cursor.set_position(offset);
                cursor.iowrite_with(team_offset as u32 + 8, scroll::BE)?;
            }
        }

        Ok(cursor.into_inner())
    }
}

impl<'a> CodeDirectoryBlob<'a> {
    /// Adjust the version of the data structure according to what fields are set.
    ///
    /// Returns the old version.
    pub fn adjust_version(&mut self) -> u32 {
        let old_version = self.version;

        let mut minimum_version = 0x20000;

        if self.scatter_offset.is_some() {
            minimum_version = CS_SUPPORTSSCATTER;
        }
        if self.team_name.is_some() {
            minimum_version = CS_SUPPORTSTEAMID;
        }
        if self.spare3.is_some() || self.code_limit_64.is_some() {
            minimum_version = CS_SUPPORTSCODELIMIT64;
        }
        if self.exec_seg_base.is_some()
            || self.exec_seg_limit.is_some()
            || self.exec_seg_flags.is_some()
        {
            minimum_version = CS_SUPPORTSEXECSEG;
        }
        if self.runtime.is_some() || self.pre_encrypt_offset.is_some() {
            minimum_version = CS_SUPPORTSRUNTIME;
        }
        if self.linkage_hash_type.is_some()
            || self.linkage_truncated.is_some()
            || self.spare4.is_some()
            || self.linkage_offset.is_some()
            || self.linkage_size.is_some()
        {
            minimum_version = CS_SUPPORTSLINKAGE;
        }

        self.version = minimum_version;

        old_version
    }

    /// Clears optional fields that are newer than the current version.
    ///
    /// The C structure is versioned and our Rust struct is a superset of
    /// all versions. While our serializer should omit too new fields for
    /// a given version, it is possible for some optional fields to be set
    /// when they wouldn't get serialized.
    ///
    /// Calling this function will set fields not present in the current
    /// version to None.
    pub fn clear_newer_fields(&mut self) {
        if self.version < CS_SUPPORTSSCATTER {
            self.scatter_offset = None;
        }
        if self.version < CS_SUPPORTSTEAMID {
            self.team_name = None;
        }
        if self.version < CS_SUPPORTSCODELIMIT64 {
            self.spare3 = None;
            self.code_limit_64 = None;
        }
        if self.version < CS_SUPPORTSEXECSEG {
            self.exec_seg_base = None;
            self.exec_seg_limit = None;
            self.exec_seg_flags = None;
        }
        if self.version < CS_SUPPORTSRUNTIME {
            self.runtime = None;
            self.pre_encrypt_offset = None;
        }
        if self.version < CS_SUPPORTSLINKAGE {
            self.linkage_hash_type = None;
            self.linkage_truncated = None;
            self.spare4 = None;
            self.linkage_offset = None;
            self.linkage_size = None;
        }
    }

    pub fn to_owned(&self) -> CodeDirectoryBlob<'static> {
        CodeDirectoryBlob {
            version: self.version,
            flags: self.flags,
            code_limit: self.code_limit,
            hash_size: self.hash_size,
            hash_type: self.hash_type,
            platform: self.platform,
            page_size: self.page_size,
            spare2: self.spare2,
            scatter_offset: self.scatter_offset,
            spare3: self.spare3,
            code_limit_64: self.code_limit_64,
            exec_seg_base: self.exec_seg_base,
            exec_seg_limit: self.exec_seg_limit,
            exec_seg_flags: self.exec_seg_flags,
            runtime: self.runtime,
            pre_encrypt_offset: self.pre_encrypt_offset,
            linkage_hash_type: self.linkage_hash_type,
            linkage_truncated: self.linkage_truncated,
            spare4: self.spare4,
            linkage_offset: self.linkage_offset,
            linkage_size: self.linkage_size,
            ident: Cow::Owned(self.ident.clone().into_owned()),
            team_name: self
                .team_name
                .as_ref()
                .map(|x| Cow::Owned(x.clone().into_owned())),
            code_hashes: self
                .code_hashes
                .iter()
                .map(|h| h.to_owned())
                .collect::<Vec<_>>(),
            special_hashes: self
                .special_hashes
                .iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect::<HashMap<_, _>>(),
        }
    }
}

/// Represents an embedded signature (CSMAGIC_EMBEDDED_SIGNATURE).
#[derive(Debug)]
pub struct EmbeddedSignatureBlob<'a> {
    data: &'a [u8],
}

impl<'a> Blob<'a> for EmbeddedSignatureBlob<'a> {
    fn magic() -> u32 {
        CSMAGIC_EMBEDDED_SIGNATURE
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, MachOError> {
        Ok(Self {
            data: read_and_validate_blob_header(data, Self::magic())?,
        })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, MachOError> {
        Ok(self.data.to_vec())
    }
}

/// An old embedded signature (CSMAGIC_EMBEDDED_SIGNATURE_OLD).
#[derive(Debug)]
pub struct EmbeddedSignatureOldBlob<'a> {
    data: &'a [u8],
}

impl<'a> Blob<'a> for EmbeddedSignatureOldBlob<'a> {
    fn magic() -> u32 {
        CSMAGIC_EMBEDDED_SIGNATURE_OLD
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, MachOError> {
        Ok(Self {
            data: read_and_validate_blob_header(data, Self::magic())?,
        })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, MachOError> {
        Ok(self.data.to_vec())
    }
}

/// Represents an Entitlements blob (CSSLOT_ENTITLEMENTS).
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
        CSMAGIC_EMBEDDED_ENTITLEMENTS
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, MachOError> {
        let data = read_and_validate_blob_header(data, Self::magic())?;
        let s = std::str::from_utf8(data)?;

        Ok(Self { plist: s.into() })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, MachOError> {
        Ok(self.plist.as_bytes().to_vec())
    }
}

impl<'a> EntitlementsBlob<'a> {
    /// Construct an instance using any string as the payload.
    pub fn from_string(s: &impl ToString) -> Self {
        Self {
            plist: s.to_string().into(),
        }
    }
}

impl<'a> std::fmt::Display for EntitlementsBlob<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.plist)
    }
}

/// A detached signature (CSMAGIC_DETACHED_SIGNATURE).
#[derive(Debug)]
pub struct DetachedSignatureBlob<'a> {
    data: &'a [u8],
}

impl<'a> Blob<'a> for DetachedSignatureBlob<'a> {
    fn magic() -> u32 {
        CSMAGIC_DETACHED_SIGNATURE
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, MachOError> {
        Ok(Self {
            data: read_and_validate_blob_header(data, Self::magic())?,
        })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, MachOError> {
        Ok(self.data.to_vec())
    }
}

/// Represents a generic blob wrapper (CSMAGIC_BLOBWRAPPER).
pub struct BlobWrapperBlob<'a> {
    data: &'a [u8],
}

impl<'a> Blob<'a> for BlobWrapperBlob<'a> {
    fn magic() -> u32 {
        CSMAGIC_BLOBWRAPPER
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, MachOError> {
        Ok(Self {
            data: read_and_validate_blob_header(data, Self::magic())?,
        })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, MachOError> {
        Ok(self.data.to_vec())
    }
}

impl<'a> std::fmt::Debug for BlobWrapperBlob<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", hex::encode(self.data)))
    }
}

impl<'a> BlobWrapperBlob<'a> {
    /// Construct an instance where the payload (post blob header) is given data.
    pub fn from_data(data: &'a [u8]) -> BlobWrapperBlob<'a> {
        Self { data }
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

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, MachOError> {
        let (magic, _, data) = read_blob_header(data)?;

        Ok(Self { magic, data })
    }

    fn serialize_payload(&self) -> Result<Vec<u8>, MachOError> {
        Ok(self.data.to_vec())
    }

    // We need to implement this for custom magic serialization.
    fn to_blob_bytes(&self) -> Result<Vec<u8>, MachOError> {
        let mut res = Vec::with_capacity(self.data.len() + 8);
        res.iowrite_with(self.magic, scroll::BE)?;
        res.iowrite_with(self.data.len() as u32 + 8, scroll::BE)?;
        res.write_all(&self.data)?;

        Ok(res)
    }
}

impl<'a> std::fmt::Debug for OtherBlob<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", hex::encode(self.data)))
    }
}

#[repr(C)]
pub struct Scatter {
    /// Number of pages. 0 for sentinel only.
    count: u32,
    /// First page number.
    base: u32,
    /// Offset in target.
    target_offset: u64,
    /// Reserved.
    spare: u64,
}

#[derive(Debug)]
pub enum MachOError {
    MissingLinkedit,
    BadMagic,
    ScrollError(scroll::Error),
    Utf8Error(std::str::Utf8Error),
    BadIdentifierString,
    BadTeamString,
    Digest(DigestError),
    Io(std::io::Error),
    Unimplemented,
    Malformed,
}

impl std::fmt::Display for MachOError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingLinkedit => f.write_fmt(format_args!(
                "unable to locate {} segment despite load command reference",
                SEG_LINKEDIT,
            )),
            Self::BadMagic => f.write_str("bad magic value in SuperBlob header"),
            Self::ScrollError(e) => e.fmt(f),
            Self::Utf8Error(e) => e.fmt(f),
            Self::BadIdentifierString => f.write_str("identifier string isn't null terminated"),
            Self::BadTeamString => f.write_str("team name string isn't null terminated"),
            Self::Digest(e) => f.write_fmt(format_args!("digest error: {}", e)),
            Self::Io(e) => f.write_fmt(format_args!("I/O error: {}", e)),
            Self::Unimplemented => f.write_str("functionality not implemented"),
            Self::Malformed => f.write_str("data is malformed"),
        }
    }
}

impl std::error::Error for MachOError {}

impl From<scroll::Error> for MachOError {
    fn from(e: scroll::Error) -> Self {
        Self::ScrollError(e)
    }
}

impl From<std::str::Utf8Error> for MachOError {
    fn from(e: std::str::Utf8Error) -> Self {
        Self::Utf8Error(e)
    }
}

impl From<DigestError> for MachOError {
    fn from(e: DigestError) -> Self {
        Self::Digest(e)
    }
}

impl From<std::io::Error> for MachOError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Describes signature data embedded within a Mach-O binary.
pub struct MachOSignatureData<'a> {
    /// The number of segments in the Mach-O binary.
    pub segments_count: usize,

    /// Which segment offset is the `__LINKEDIT` segment.
    pub linkedit_segment_index: usize,

    /// Start offset of `__LINKEDIT` segment within the binary.
    pub linkedit_segment_start_offset: usize,

    /// End offset of `__LINKEDIT` segment within the binary.
    pub linkedit_segment_end_offset: usize,

    /// Start offset of signature data in `__LINKEDIT` within the binary.
    pub linkedit_signature_start_offset: usize,

    /// End offset of signature data in `__LINKEDIT` within the binary.
    pub linkedit_signature_end_offset: usize,

    /// The start offset of the signature data within the `__LINKEDIT` segment.
    pub signature_start_offset: usize,

    /// The end offset of the signature data within the `__LINKEDIT` segment.
    pub signature_end_offset: usize,

    /// Raw data in the `__LINKEDIT` segment.
    pub linkedit_segment_data: &'a [u8],

    /// The signature data within the `__LINKEDIT` segment.
    pub signature_data: &'a [u8],
}

/// Attempt to extract a reference to raw signature data in a Mach-O binary.
///
/// An `LC_CODE_SIGNATURE` load command in the Mach-O file header points to
/// signature data in the `__LINKEDIT` segment.
///
/// This function is used as part of parsing signature data. You probably want to
/// use a function that parses referenced data.
pub fn find_signature_data<'a>(
    obj: &'a MachO,
) -> Result<Option<MachOSignatureData<'a>>, MachOError> {
    if let Some(linkedit_data_command) = obj.load_commands.iter().find_map(|load_command| {
        if let CommandVariant::CodeSignature(command) = &load_command.command {
            Some(command)
        } else {
            None
        }
    }) {
        // Now find the slice of data in the __LINKEDIT segment we need to parse.
        let (linkedit_segment_index, linkedit) = obj
            .segments
            .iter()
            .enumerate()
            .find(|(_, segment)| {
                if let Ok(name) = segment.name() {
                    name == SEG_LINKEDIT
                } else {
                    false
                }
            })
            .ok_or(MachOError::MissingLinkedit)?;

        let linkedit_segment_start_offset = linkedit.fileoff as usize;
        let linkedit_segment_end_offset = linkedit_segment_start_offset + linkedit.data.len();
        let linkedit_signature_start_offset = linkedit_data_command.dataoff as usize;
        let linkedit_signature_end_offset =
            linkedit_signature_start_offset + linkedit_data_command.datasize as usize;
        let signature_start_offset =
            linkedit_data_command.dataoff as usize - linkedit.fileoff as usize;
        let signature_end_offset = signature_start_offset + linkedit_data_command.datasize as usize;

        let signature_data = &linkedit.data[signature_start_offset..signature_end_offset];

        Ok(Some(MachOSignatureData {
            segments_count: obj.segments.len(),
            linkedit_segment_index,
            linkedit_segment_start_offset,
            linkedit_segment_end_offset,
            linkedit_signature_start_offset,
            linkedit_signature_end_offset,
            signature_start_offset,
            signature_end_offset,
            linkedit_segment_data: linkedit.data,
            signature_data,
        }))
    } else {
        Ok(None)
    }
}

/// Parse raw Mach-O signature data into a data structure.
///
/// The source data likely came from the `__LINKEDIT` segment and was
/// discovered via `find_signature_data()`.
///
/// Only a high-level parse of the super blob and its blob indices is performed:
/// the parser does not look inside individual blob payloads.
pub fn parse_signature_data(data: &[u8]) -> Result<EmbeddedSignature<'_>, MachOError> {
    let magic: u32 = data.pread_with(0, scroll::BE)?;

    if magic == CSMAGIC_EMBEDDED_SIGNATURE {
        EmbeddedSignature::from_bytes(data)
    } else {
        Err(MachOError::BadMagic)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        cryptographic_message_syntax::SignedData,
        std::{
            io::Read,
            path::{Path, PathBuf},
        },
    };

    const MACHO_UNIVERSAL_MAGIC: [u8; 4] = [0xca, 0xfe, 0xba, 0xbe];
    const MACHO_64BIT_MAGIC: [u8; 4] = [0xfe, 0xed, 0xfa, 0xcf];

    /// Find files in a directory appearing to be Mach-O by sniffing magic.
    ///
    /// Ignores file I/O errors.
    fn find_likely_macho_files(path: &Path) -> Vec<PathBuf> {
        let mut res = Vec::new();

        let dir = std::fs::read_dir(path).unwrap();

        for entry in dir {
            let entry = entry.unwrap();

            if let Ok(mut fh) = std::fs::File::open(&entry.path()) {
                let mut magic = [0; 4];

                if let Ok(size) = fh.read(&mut magic) {
                    if size == 4 && (magic == MACHO_UNIVERSAL_MAGIC || magic == MACHO_64BIT_MAGIC) {
                        res.push(entry.path());
                    }
                }
            }
        }

        res
    }

    fn find_apple_embedded_signature<'a>(
        macho: &'a goblin::mach::MachO,
    ) -> Option<EmbeddedSignature<'a>> {
        if let Ok(Some(codesign_data)) = find_signature_data(macho) {
            if let Ok(signature) = parse_signature_data(codesign_data.signature_data) {
                Some(signature)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn validate_macho(path: &Path, macho: &MachO) {
        // We found signature data in the binary.
        if let Some(signature) = find_apple_embedded_signature(macho) {
            // Attempt a deep parse of all blobs.
            for blob in &signature.blobs {
                match blob.clone().into_parsed_blob() {
                    Ok(parsed) => {
                        // Attempt to roundtrip the blob data.
                        match parsed.blob.to_blob_bytes() {
                            Ok(serialized) => {
                                if serialized != blob.data {
                                    println!("blob serialization roundtrip failure on {}: index {}, magic {:?}",
                                        path.display(),
                                        blob.index,
                                        blob.magic,
                                    );
                                }
                            }
                            Err(e) => {
                                println!(
                                    "blob serialization failure on {}; index {}, magic {:?}: {}",
                                    path.display(),
                                    blob.index,
                                    blob.magic,
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!(
                            "blob parse failure on {}; index {}, magic {:?}: {}",
                            path.display(),
                            blob.index,
                            blob.magic,
                            e
                        );
                    }
                }
            }

            // Found a CMS signed data blob.
            if let Ok(Some(cms)) = signature.signature_data() {
                match SignedData::parse_ber(&cms) {
                    Ok(signed_data) => {
                        for signer in signed_data.signers() {
                            if let Err(e) = signer.verify_signature_with_signed_data(&signed_data) {
                                println!(
                                    "signature verification failed for {}: {}",
                                    path.display(),
                                    e
                                );
                            }

                            if let Ok(()) =
                                signer.verify_message_digest_with_signed_data(&signed_data)
                            {
                                println!(
                                    "message digest verification unexpectedly correct for {}",
                                    path.display()
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!("error performing CMS parse of {}: {:?}", path.display(), e);
                    }
                }
            }
        }
    }

    fn validate_macho_in_dir(dir: &Path) {
        for path in find_likely_macho_files(dir).into_iter() {
            if let Ok(file_data) = std::fs::read(&path) {
                if let Ok(mach) = goblin::mach::Mach::parse(&file_data) {
                    match mach {
                        goblin::mach::Mach::Binary(macho) => {
                            validate_macho(&path, &macho);
                        }
                        goblin::mach::Mach::Fat(multiarch) => {
                            for i in 0..multiarch.narches {
                                if let Ok(macho) = multiarch.get(i) {
                                    validate_macho(&path, &macho);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn parse_applications_macho_signatures() {
        // This test scans common directories containing Mach-O files on macOS and
        // verifies we can parse CMS blobs within.

        if let Ok(dir) = std::fs::read_dir("/Applications") {
            for entry in dir {
                let entry = entry.unwrap();

                let search_dir = entry.path().join("Contents").join("MacOS");

                if search_dir.exists() {
                    validate_macho_in_dir(&search_dir);
                }
            }
        }

        for dir in &["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin"] {
            let dir = PathBuf::from(dir);

            if dir.exists() {
                validate_macho_in_dir(&dir);
            }
        }
    }
}
