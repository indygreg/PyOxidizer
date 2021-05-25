// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use scroll::{IOread, Pread, SizeWith};

/// A XAR archive header.
///
/// The header effectively defines a table of contents, which
/// holds information about the content of the archive.
#[derive(Clone, Copy, Debug, IOread, Pread, SizeWith)]
pub struct XarHeader {
    /// File magic. `xar!`.
    pub magic: u32,

    /// Size of this header + magic.
    pub size: u16,

    /// Format version number.
    pub version: u16,

    /// Size in bytes of zlib compressed table of contents.
    pub toc_length_compressed: u64,

    /// Size in bytes of uncompressed table of contents.
    pub toc_length_uncompressed: u64,

    /// Checksum algorithm used.
    pub checksum_algorithm_id: u32,
}

/// Checksum format used in file.
pub enum XarChecksum {
    None,
    Sha1,
    Md5,
    Sha256,
    Sha512,
    Other(u32),
}

impl From<u32> for XarChecksum {
    fn from(i: u32) -> Self {
        match i {
            0 => Self::None,
            1 => Self::Sha1,
            2 => Self::Md5,
            3 => Self::Sha256,
            4 => Self::Sha512,
            _ => Self::Other(i),
        }
    }
}
