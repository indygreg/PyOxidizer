// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! .deb file reading functionality. */

use {
    crate::deb::{DebError, Result},
    std::{
        io::Read,
        ops::{Deref, DerefMut},
    },
};

fn reader_from_filename(extension: &str, data: std::io::Cursor<Vec<u8>>) -> Result<Box<dyn Read>> {
    match extension {
        "" => Ok(Box::new(data)),
        ".gz" => Ok(Box::new(libflate::gzip::Decoder::new(data)?)),
        ".xz" => Ok(Box::new(xz2::read::XzDecoder::new(data))),
        ".zst" => Ok(Box::new(zstd::Decoder::new(data)?)),
        _ => Err(DebError::UnknownCompression(extension.to_string())),
    }
}

/// A reader of .deb files.
///
/// A .deb binary package file is an ar archive with 3 entries:
///
/// 1. `debian-binary` holding the version of the binary package format.
/// 2. `control.tar` holding package metadata.
/// 3. `data.tar[.<ext>]` holding file content.
pub struct BinaryPackageReader<R: Read> {
    archive: ar::Archive<R>,
}

impl<R: Read> BinaryPackageReader<R> {
    /// Construct a new instance from a reader.
    pub fn new(reader: R) -> Result<Self> {
        Ok(Self {
            archive: ar::Archive::new(reader),
        })
    }

    /// Obtain the next entry from the underlying ar archive.
    ///
    /// The entry will be converted to an enum that richly represents its content.
    pub fn next_entry(&mut self) -> Option<Result<BinaryPackageEntry>> {
        if let Some(entry) = self.archive.next_entry() {
            match entry {
                Ok(mut entry) => {
                    // We could do this in the domain of bytes. But filenames should be ASCII,
                    // so converting to strings feels reasonably safe.
                    let filename = String::from_utf8_lossy(entry.header().identifier()).to_string();

                    let mut data = vec![];
                    match entry.read_to_end(&mut data) {
                        Ok(_) => {}
                        Err(e) => {
                            return Some(Err(e.into()));
                        }
                    }

                    let data = std::io::Cursor::new(data);

                    if filename == "debian-binary" {
                        Some(Ok(BinaryPackageEntry::DebianBinary(data)))
                    } else if let Some(tail) = filename.strip_prefix("control.tar") {
                        match reader_from_filename(tail, data) {
                            Ok(res) => Some(Ok(BinaryPackageEntry::Control(ControlTarReader {
                                archive: tar::Archive::new(res),
                            }))),
                            Err(e) => Some(Err(e)),
                        }
                    } else if let Some(tail) = filename.strip_prefix("data.tar") {
                        match reader_from_filename(tail, data) {
                            Ok(res) => Some(Ok(BinaryPackageEntry::Data(DataTarReader {
                                archive: tar::Archive::new(res),
                            }))),
                            Err(e) => Some(Err(e)),
                        }
                    } else {
                        Some(Err(DebError::UnknownBinaryPackageEntry(
                            filename.to_string(),
                        )))
                    }
                }
                Err(e) => Some(Err(e.into())),
            }
        } else {
            None
        }
    }
}

/// Represents an entry in a .deb archive.
pub enum BinaryPackageEntry {
    /// The `debian-binary` file.
    DebianBinary(std::io::Cursor<Vec<u8>>),
    /// The `control.tar` tar archive.
    Control(ControlTarReader),
    /// The `data.tar[.<ext>]` tar archive.
    Data(DataTarReader),
}

/// A reader for `control.tar` files.
pub struct ControlTarReader {
    archive: tar::Archive<Box<dyn Read>>,
}

impl Deref for ControlTarReader {
    type Target = tar::Archive<Box<dyn Read>>;

    fn deref(&self) -> &Self::Target {
        &self.archive
    }
}

impl DerefMut for ControlTarReader {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.archive
    }
}

/// A reader for `data.tar` files.
pub struct DataTarReader {
    archive: tar::Archive<Box<dyn Read>>,
}

impl Deref for DataTarReader {
    type Target = tar::Archive<Box<dyn Read>>;

    fn deref(&self) -> &Self::Target {
        &self.archive
    }
}

impl DerefMut for DataTarReader {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.archive
    }
}
