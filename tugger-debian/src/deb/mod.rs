// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Interfaces for .deb package files.

The .deb file specification lives at <https://manpages.debian.org/unstable/dpkg-dev/deb.5.en.html>.
*/

use {
    crate::{
        binary_package_control::BinaryPackageControlFile, control::ControlError,
        deb::reader::resolve_control_file, repository::release::ChecksumType,
    },
    std::io::Read,
    thiserror::Error,
    tugger_file_manifest::FileManifestError,
};

pub mod builder;
pub mod reader;

/// Represents an error related to .deb file handling.
#[derive(Debug, Error)]
pub enum DebError {
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("path error: {0}")]
    PathError(String),
    #[error("file manifest error: {0}")]
    FileManifestError(#[from] FileManifestError),
    #[error("control file error: {0:?}")]
    Control(#[from] ControlError),
    #[error("Unknown binary package entry: {0}")]
    UnknownBinaryPackageEntry(String),
    #[error("Unknown compression for filename: {0}")]
    UnknownCompression(String),
    #[error("Control file lacks a paragraph")]
    ControlFileNoParagraph,
    #[error("Control file not found")]
    ControlFileNotFound,
}

impl<W> From<std::io::IntoInnerError<W>> for DebError {
    fn from(e: std::io::IntoInnerError<W>) -> Self {
        Self::IoError(e.into())
    }
}

/// Result type for .deb functionality.
pub type Result<T> = std::result::Result<T, DebError>;

/// Compression format to apply to `.deb` files.
pub enum DebCompression {
    /// Do not compress contents of `.deb` files.
    Uncompressed,
    /// Compress as `.gz` files.
    Gzip,
    /// Compress as `.xz` files using a specified compression level.
    Xz(u32),
    /// Compress as `.zst` files using a specified compression level.
    Zstandard(i32),
}

impl DebCompression {
    /// Obtain the filename extension for this compression format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Uncompressed => "",
            Self::Gzip => ".gz",
            Self::Xz(_) => ".xz",
            Self::Zstandard(_) => ".zst",
        }
    }

    /// Compress input data from a reader.
    pub fn compress(&self, reader: &mut impl Read) -> Result<Vec<u8>> {
        let mut buffer = vec![];

        match self {
            Self::Uncompressed => {
                std::io::copy(reader, &mut buffer)?;
            }
            Self::Gzip => {
                let header = libflate::gzip::HeaderBuilder::new().finish();

                let mut encoder = libflate::gzip::Encoder::with_options(
                    &mut buffer,
                    libflate::gzip::EncodeOptions::new().header(header),
                )?;
                std::io::copy(reader, &mut encoder)?;
                encoder.finish().into_result()?;
            }
            Self::Xz(level) => {
                let mut encoder = xz2::write::XzEncoder::new(buffer, *level);
                std::io::copy(reader, &mut encoder)?;
                buffer = encoder.finish()?;
            }
            Self::Zstandard(level) => {
                let mut encoder = zstd::Encoder::new(buffer, *level)?;
                std::io::copy(reader, &mut encoder)?;
                buffer = encoder.finish()?;
            }
        }

        Ok(buffer)
    }
}

/// Describes a reference to a `.deb` Debian package existing somewhere.
///
/// This trait is used as a generic way to refer to a `.deb` package, without implementations
/// necessarily having immediate access to the full content/data of that `.deb` package.
pub trait DebPackageReference {
    /// Obtain the size in bytes of the `.deb` file.
    ///
    /// This becomes the `Size` field in `Packages*` control files.
    fn size_bytes(&self) -> usize;

    /// Obtains the binary digest of this file given a checksum flavor.
    ///
    /// Implementations can compute the digest at run-time or return a cached value.
    fn digest(&self, checksum: ChecksumType) -> Result<Vec<u8>>;

    /// Obtain the filename of this `.deb`.
    ///
    /// This should be just the file name, without any directory components.
    fn filename(&self) -> String;

    /// Obtain the parsed `control` file from the `control.tar` file inside the `.deb`.
    fn control_file<'a>(&self) -> Result<BinaryPackageControlFile<'a>>;
}

/// Holds the content of a `.deb` file in-memory.
pub struct InMemoryDebFile {
    filename: String,
    data: Vec<u8>,
}

impl InMemoryDebFile {
    /// Create a new instance bound to memory.
    pub fn new(filename: String, data: Vec<u8>) -> Self {
        Self { filename, data }
    }
}

impl DebPackageReference for InMemoryDebFile {
    fn size_bytes(&self) -> usize {
        self.data.len()
    }

    fn digest(&self, checksum: ChecksumType) -> Result<Vec<u8>> {
        let mut h = checksum.new_hasher();
        h.update(&self.data);

        Ok(h.finish().to_vec())
    }

    fn filename(&self) -> String {
        self.filename.clone()
    }

    fn control_file<'a>(&self) -> Result<BinaryPackageControlFile<'a>> {
        resolve_control_file(std::io::Cursor::new(&self.data))
    }
}
