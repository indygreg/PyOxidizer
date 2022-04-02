// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Code directory data structure and related types.

use {
    crate::{
        embedded_signature::{
            read_and_validate_blob_header, Blob, CodeSigningMagic, CodeSigningSlot, Digest,
            DigestType,
        },
        error::AppleCodesignError,
        macho::{MachoTarget, Platform},
    },
    scroll::{IOwrite, Pread},
    semver::Version,
    std::{borrow::Cow, collections::HashMap, io::Write, str::FromStr},
};

bitflags::bitflags! {
    /// Code signature flags.
    ///
    /// These flags are embedded in the Code Directory and govern use of the embedded
    /// signature.
    pub struct CodeSignatureFlags: u32 {
        /// Code may act as a host that controls and supervises guest code.
        const HOST = 0x0001;
        /// The code has been sealed without a signing identity.
        const ADHOC = 0x0002;
        /// Set the "hard" status bit for the code when it starts running.
        const FORCE_HARD = 0x0100;
        /// Implicitly set the "kill" status bit for the code when it starts running.
        const FORCE_KILL = 0x0200;
        /// Force certificate expiration checks.
        const FORCE_EXPIRATION = 0x0400;
        /// Restrict dyld loading.
        const RESTRICT = 0x0800;
        /// Enforce code signing.
        const ENFORCEMENT = 0x1000;
        /// Library validation required.
        const LIBRARY_VALIDATION = 0x2000;
        /// Apply runtime hardening policies.
        const RUNTIME = 0x10000;
        /// The code was automatically signed by the linker.
        ///
        /// This signature should be ignored in any new signing operation.
        const LINKER_SIGNED = 0x20000;
    }
}

impl FromStr for CodeSignatureFlags {
    type Err = AppleCodesignError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "host" => Ok(Self::HOST),
            "hard" => Ok(Self::FORCE_HARD),
            "kill" => Ok(Self::FORCE_KILL),
            "expires" => Ok(Self::FORCE_EXPIRATION),
            "library" => Ok(Self::LIBRARY_VALIDATION),
            "runtime" => Ok(Self::RUNTIME),
            "linker-signed" => Ok(Self::LINKER_SIGNED),
            _ => Err(AppleCodesignError::CodeSignatureUnknownFlag(s.to_string())),
        }
    }
}

impl CodeSignatureFlags {
    /// Attempt to convert a series of strings into a [CodeSignatureFlags].
    pub fn from_strs(s: &[&str]) -> Result<CodeSignatureFlags, AppleCodesignError> {
        let mut flags = CodeSignatureFlags::empty();

        for s in s {
            flags |= Self::from_str(s)?;
        }

        Ok(flags)
    }
}

bitflags::bitflags! {
    /// Flags that influence behavior of executable segment.
    pub struct ExecutableSegmentFlags: u64 {
        /// Executable segment belongs to main binary.
        const MAIN_BINARY = 0x0001;
        /// Allow unsigned pages (for debugging).
        const ALLOW_UNSIGNED = 0x0010;
        /// Main binary is debugger.
        const DEBUGGER = 0x0020;
        /// JIT enabled.
        const JIT = 0x0040;
        /// Skip library validation (obsolete).
        const SKIP_LIBRARY_VALIDATION = 0x0080;
        /// Can bless code directory hash for execution.
        const CAN_LOAD_CD_HASH = 0x0100;
        /// Can execute blessed code directory hash.
        const CAN_EXEC_CD_HASH = 0x0200;
    }
}

impl FromStr for ExecutableSegmentFlags {
    type Err = AppleCodesignError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "main-binary" => Ok(Self::MAIN_BINARY),
            "allow-unsigned" => Ok(Self::ALLOW_UNSIGNED),
            "debugger" => Ok(Self::DEBUGGER),
            "jit" => Ok(Self::JIT),
            "skip-library-validation" => Ok(Self::SKIP_LIBRARY_VALIDATION),
            "can-load-cd-hash" => Ok(Self::CAN_LOAD_CD_HASH),
            "can-exec-cd-hash" => Ok(Self::CAN_EXEC_CD_HASH),
            _ => Err(AppleCodesignError::ExecutableSegmentUnknownFlag(
                s.to_string(),
            )),
        }
    }
}

/// Version of Code Directory data structure.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum CodeDirectoryVersion {
    Initial = 0x20000,
    SupportsScatter = 0x20100,
    SupportsTeamId = 0x20200,
    SupportsCodeLimit64 = 0x20300,
    SupportsExecutableSegment = 0x20400,
    SupportsRuntime = 0x20500,
    SupportsLinkage = 0x20600,
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

fn get_hashes(data: &[u8], offset: usize, count: usize, hash_size: usize) -> Vec<Digest<'_>> {
    data[offset..offset + (count * hash_size)]
        .chunks(hash_size)
        .map(|data| Digest { data: data.into() })
        .collect()
}

/// Represents a code directory blob entry.
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
    pub flags: CodeSignatureFlags,
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
    pub exec_seg_flags: Option<ExecutableSegmentFlags>,
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
        u32::from(CodeSigningMagic::CodeDirectory)
    }

    fn from_blob_bytes(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        read_and_validate_blob_header(data, Self::magic(), "code directory blob")?;

        let offset = &mut 8;

        let version = data.gread_with(offset, scroll::BE)?;
        let flags = data.gread_with::<u32>(offset, scroll::BE)?;
        let flags = unsafe { CodeSignatureFlags::from_bits_unchecked(flags) };
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

        let scatter_offset = if version >= CodeDirectoryVersion::SupportsScatter as u32 {
            let v = data.gread_with(offset, scroll::BE)?;

            if v != 0 {
                Some(v)
            } else {
                None
            }
        } else {
            None
        };
        let team_offset = if version >= CodeDirectoryVersion::SupportsTeamId as u32 {
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

        let (spare3, code_limit_64) = if version >= CodeDirectoryVersion::SupportsCodeLimit64 as u32
        {
            (
                Some(data.gread_with(offset, scroll::BE)?),
                Some(data.gread_with(offset, scroll::BE)?),
            )
        } else {
            (None, None)
        };

        let (exec_seg_base, exec_seg_limit, exec_seg_flags) =
            if version >= CodeDirectoryVersion::SupportsExecutableSegment as u32 {
                assert_eq!(*offset, 0x40);
                (
                    Some(data.gread_with(offset, scroll::BE)?),
                    Some(data.gread_with(offset, scroll::BE)?),
                    Some(data.gread_with::<u64>(offset, scroll::BE)?),
                )
            } else {
                (None, None, None)
            };

        let exec_seg_flags = exec_seg_flags
            .map(|flags| unsafe { ExecutableSegmentFlags::from_bits_unchecked(flags) });

        let (runtime, pre_encrypt_offset) =
            if version >= CodeDirectoryVersion::SupportsRuntime as u32 {
                assert_eq!(*offset, 0x58);
                (
                    Some(data.gread_with(offset, scroll::BE)?),
                    Some(data.gread_with(offset, scroll::BE)?),
                )
            } else {
                (None, None)
            };

        let (linkage_hash_type, linkage_truncated, spare4, linkage_offset, linkage_size) =
            if version >= CodeDirectoryVersion::SupportsLinkage as u32 {
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
            Some(res) => {
                Cow::from(res.map_err(|_| AppleCodesignError::CodeDirectoryMalformedIdentifier)?)
            }
            None => {
                return Err(AppleCodesignError::CodeDirectoryMalformedIdentifier);
            }
        };

        let team_name = if let Some(team_offset) = team_offset {
            match data[team_offset as usize..]
                .split(|&b| b == 0)
                .map(std::str::from_utf8)
                .next()
            {
                Some(res) => {
                    Some(Cow::from(res.map_err(|_| {
                        AppleCodesignError::CodeDirectoryMalformedTeam
                    })?))
                }
                None => {
                    return Err(AppleCodesignError::CodeDirectoryMalformedTeam);
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

    fn serialize_payload(&self) -> Result<Vec<u8>, AppleCodesignError> {
        let mut cursor = std::io::Cursor::new(Vec::<u8>::new());

        // We need to do this in 2 phases because we don't know the length until
        // we build up the data structure.

        cursor.iowrite_with(self.version, scroll::BE)?;
        cursor.iowrite_with(self.flags.bits, scroll::BE)?;
        let hash_offset_cursor_position = cursor.position();
        cursor.iowrite_with(0u32, scroll::BE)?;
        let ident_offset_cursor_position = cursor.position();
        cursor.iowrite_with(0u32, scroll::BE)?;
        assert_eq!(cursor.position(), 0x10);

        // Hash offsets and counts are wonky. The recorded hash offset is the beginning
        // of code hashes and special hashes are in "negative" indices before
        // that offset. Hashes are also at the index of their CodeSigningSlot constant.
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

        if self.version >= CodeDirectoryVersion::SupportsScatter as u32 {
            scatter_offset_cursor_position = Some(cursor.position());
            cursor.iowrite_with(self.scatter_offset.unwrap_or(0), scroll::BE)?;

            if self.version >= CodeDirectoryVersion::SupportsTeamId as u32 {
                team_offset_cursor_position = Some(cursor.position());
                cursor.iowrite_with(0u32, scroll::BE)?;

                if self.version >= CodeDirectoryVersion::SupportsCodeLimit64 as u32 {
                    cursor.iowrite_with(self.spare3.unwrap_or(0), scroll::BE)?;
                    assert_eq!(cursor.position(), 0x30);
                    cursor.iowrite_with(self.code_limit_64.unwrap_or(0), scroll::BE)?;

                    if self.version >= CodeDirectoryVersion::SupportsExecutableSegment as u32 {
                        cursor.iowrite_with(self.exec_seg_base.unwrap_or(0), scroll::BE)?;
                        assert_eq!(cursor.position(), 0x40);
                        cursor.iowrite_with(self.exec_seg_limit.unwrap_or(0), scroll::BE)?;
                        cursor.iowrite_with(
                            self.exec_seg_flags
                                .unwrap_or_else(ExecutableSegmentFlags::empty)
                                .bits,
                            scroll::BE,
                        )?;

                        if self.version >= CodeDirectoryVersion::SupportsRuntime as u32 {
                            assert_eq!(cursor.position(), 0x50);
                            cursor.iowrite_with(self.runtime.unwrap_or(0), scroll::BE)?;
                            cursor
                                .iowrite_with(self.pre_encrypt_offset.unwrap_or(0), scroll::BE)?;

                            if self.version >= CodeDirectoryVersion::SupportsLinkage as u32 {
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
            // .special_hashes is public and not all values are allowed. So check for
            // garbage.
            let slot = CodeSigningSlot::from(slot_index);
            assert!(
                slot.is_code_directory_specials_expressible(),
                "slot is expressible in code directory special digests"
            );

            if let Some(hash) = self.special_hashes.get(&slot) {
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
            return Err(AppleCodesignError::Unimplemented("scatter offset"));
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
    pub fn adjust_version(&mut self, target: Option<MachoTarget>) -> u32 {
        let old_version = self.version;

        let mut minimum_version = CodeDirectoryVersion::Initial;

        if self.scatter_offset.is_some() {
            minimum_version = CodeDirectoryVersion::SupportsScatter;
        }
        if self.team_name.is_some() {
            minimum_version = CodeDirectoryVersion::SupportsTeamId;
        }
        if self.spare3.is_some() || self.code_limit_64.is_some() {
            minimum_version = CodeDirectoryVersion::SupportsCodeLimit64;
        }
        if self.exec_seg_base.is_some()
            || self.exec_seg_limit.is_some()
            || self.exec_seg_flags.is_some()
        {
            minimum_version = CodeDirectoryVersion::SupportsExecutableSegment;
        }
        if self.runtime.is_some() || self.pre_encrypt_offset.is_some() {
            minimum_version = CodeDirectoryVersion::SupportsRuntime;
        }
        if self.linkage_hash_type.is_some()
            || self.linkage_truncated.is_some()
            || self.spare4.is_some()
            || self.linkage_offset.is_some()
            || self.linkage_size.is_some()
        {
            minimum_version = CodeDirectoryVersion::SupportsLinkage;
        }

        // Some platforms have hard requirements for the minimum version. If
        // targeting settings are in effect, we raise the minimum version accordingly.
        if let Some(target) = target {
            let target_minimum = match target.platform {
                // iOS >= 15 requires a modern code signature format.
                Platform::IOs | Platform::IosSimulator => {
                    if target.minimum_os_version >= Version::new(15, 0, 0) {
                        CodeDirectoryVersion::SupportsExecutableSegment
                    } else {
                        CodeDirectoryVersion::Initial
                    }
                }
                // Let's bump the minimum version for macOS 12 out of principle.
                Platform::MacOs => {
                    if target.minimum_os_version >= Version::new(12, 0, 0) {
                        CodeDirectoryVersion::SupportsExecutableSegment
                    } else {
                        CodeDirectoryVersion::Initial
                    }
                }
                _ => CodeDirectoryVersion::Initial,
            };

            if target_minimum as u32 > minimum_version as u32 {
                minimum_version = target_minimum;
            }
        }

        self.version = minimum_version as u32;

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
        if self.version < CodeDirectoryVersion::SupportsScatter as u32 {
            self.scatter_offset = None;
        }
        if self.version < CodeDirectoryVersion::SupportsTeamId as u32 {
            self.team_name = None;
        }
        if self.version < CodeDirectoryVersion::SupportsCodeLimit64 as u32 {
            self.spare3 = None;
            self.code_limit_64 = None;
        }
        if self.version < CodeDirectoryVersion::SupportsExecutableSegment as u32 {
            self.exec_seg_base = None;
            self.exec_seg_limit = None;
            self.exec_seg_flags = None;
        }
        if self.version < CodeDirectoryVersion::SupportsRuntime as u32 {
            self.runtime = None;
            self.pre_encrypt_offset = None;
        }
        if self.version < CodeDirectoryVersion::SupportsLinkage as u32 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_signature_flags_from_str() {
        assert_eq!(
            CodeSignatureFlags::from_str("host").unwrap(),
            CodeSignatureFlags::HOST
        );
        assert_eq!(
            CodeSignatureFlags::from_str("hard").unwrap(),
            CodeSignatureFlags::FORCE_HARD
        );
        assert_eq!(
            CodeSignatureFlags::from_str("kill").unwrap(),
            CodeSignatureFlags::FORCE_KILL
        );
        assert_eq!(
            CodeSignatureFlags::from_str("expires").unwrap(),
            CodeSignatureFlags::FORCE_EXPIRATION
        );
        assert_eq!(
            CodeSignatureFlags::from_str("library").unwrap(),
            CodeSignatureFlags::LIBRARY_VALIDATION
        );
        assert_eq!(
            CodeSignatureFlags::from_str("runtime").unwrap(),
            CodeSignatureFlags::RUNTIME
        );
        assert_eq!(
            CodeSignatureFlags::from_str("linker-signed").unwrap(),
            CodeSignatureFlags::LINKER_SIGNED
        );
    }
}
