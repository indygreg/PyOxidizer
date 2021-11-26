// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Interfaces for .deb package files.

The .deb file specification lives at https://manpages.debian.org/unstable/dpkg-dev/deb.5.en.html.
*/

use {std::io::Read, thiserror::Error, tugger_file_manifest::FileManifestError};

pub mod builder;

/// Represents an error related to .deb file handling.
#[derive(Debug, Error)]
pub enum DebError {
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("path error: {0}")]
    PathError(String),
    #[error("file manifest error: {0}")]
    FileManifestError(#[from] FileManifestError),
}

impl<W> From<std::io::IntoInnerError<W>> for DebError {
    fn from(e: std::io::IntoInnerError<W>) -> Self {
        Self::IoError(e.into())
    }
}

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
    pub fn compress(&self, reader: &mut impl Read) -> Result<Vec<u8>, DebError> {
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
