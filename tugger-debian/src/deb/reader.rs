// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! .deb file reading functionality. */

use {
    crate::{
        binary_package_control::BinaryPackageControlFile,
        control::ControlParagraphReader,
        deb::{DebError, Result},
    },
    std::{
        io::{Cursor, Read},
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

impl ControlTarReader {
    /// Obtain the entries in the `control.tar` file.
    ///
    /// This can only be called once, immediately after the reader/archive is opened.
    /// It is a glorified wrapper around [tar::Archive::entries()] and has the same
    /// semantics.
    pub fn entries(&mut self) -> Result<ControlTarEntries<'_>> {
        let entries = self.archive.entries()?;

        Ok(ControlTarEntries { entries })
    }
}

/// Represents entries in a `control.tar` file.
///
/// Ideally this type wouldn't exist. It is a glorified wrapper around
/// [tar::Entries] that is needed to placate the borrow checker.
pub struct ControlTarEntries<'a> {
    entries: tar::Entries<'a, Box<dyn Read>>,
}

impl<'a> Iterator for ControlTarEntries<'a> {
    type Item = Result<ControlTarEntry<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.entries.next() {
            Some(Ok(entry)) => Some(Ok(ControlTarEntry { inner: entry })),
            Some(Err(e)) => Some(Err(e.into())),
            None => None,
        }
    }
}

/// A wrapper around [tar::Entry] for representing content in `control.tar` files.
///
/// Facilitates access to the raw [tar::Entry] as well as for obtaining a higher
/// level type that decodes known files within `control.tar` files.
pub struct ControlTarEntry<'a> {
    inner: tar::Entry<'a, Box<dyn Read>>,
}

impl<'a> Deref for ControlTarEntry<'a> {
    type Target = tar::Entry<'a, Box<dyn Read>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> DerefMut for ControlTarEntry<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a> ControlTarEntry<'a> {
    /// Attempt to convert this tar entry to a [ControlTarFile].
    ///
    ///
    pub fn to_control_file(&mut self) -> Result<(&'_ tar::Header, ControlTarFile)> {
        let path_bytes = self.inner.path_bytes().to_vec();
        let path = String::from_utf8_lossy(&path_bytes);

        let mut data = vec![];
        self.inner.read_to_end(&mut data)?;

        match path.trim_start_matches("./") {
            "control" => {
                let mut reader = ControlParagraphReader::new(Cursor::new(data));
                let paragraph = reader.next().ok_or(DebError::ControlFileNoParagraph)??;
                let control = BinaryPackageControlFile::from(paragraph);

                Ok((self.inner.header(), ControlTarFile::Control(control)))
            }
            "conffiles" => Ok((self.inner.header(), ControlTarFile::Conffiles(data))),
            "triggers" => Ok((self.inner.header(), ControlTarFile::Triggers(data))),
            "shlibs" => Ok((self.inner.header(), ControlTarFile::Shlibs(data))),
            "symbols" => Ok((self.inner.header(), ControlTarFile::Symbols(data))),
            "preinst" => Ok((self.inner.header(), ControlTarFile::Preinst(data))),
            "postinst" => Ok((self.inner.header(), ControlTarFile::Postinst(data))),
            "prerm" => Ok((self.inner.header(), ControlTarFile::Prerm(data))),
            "postrm" => Ok((self.inner.header(), ControlTarFile::Postrm(data))),
            _ => Ok((self.inner.header(), ControlTarFile::Other(path_bytes, data))),
        }
    }
}

/// Represents a parsed file in a `control.tar` archive.
///
/// Each variant encodes a known file in a `control.tar` archive.
pub enum ControlTarFile {
    /// The `control` file.
    Control(BinaryPackageControlFile<'static>),

    /// The `conffiles` file.
    Conffiles(Vec<u8>),

    /// The `triggers` file.
    Triggers(Vec<u8>),

    /// The `shlibs` file.
    Shlibs(Vec<u8>),

    /// The `symbols` file.
    Symbols(Vec<u8>),

    /// The `preinst` file.
    Preinst(Vec<u8>),

    /// The `postinst` file.
    Postinst(Vec<u8>),

    /// The `prerm` file.
    Prerm(Vec<u8>),

    /// The `postrm` file.
    Postrm(Vec<u8>),

    /// An unclassified file.
    ///
    /// First element is the path name as bytes. Second is the raw file content.
    Other(Vec<u8>, Vec<u8>),
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

/// Resolve the `control` file from the `control.tar` file within a `.deb` archive.
pub fn resolve_control_file(reader: impl Read) -> Result<BinaryPackageControlFile<'static>> {
    let mut reader = BinaryPackageReader::new(reader)?;

    while let Some(entry) = reader.next_entry() {
        if let BinaryPackageEntry::Control(mut control) = entry? {
            let mut entries = control.entries()?;

            while let Some(entry) = entries.next() {
                if let ControlTarFile::Control(control) = entry?.to_control_file()?.1 {
                    return Ok(control);
                }
            }
        }
    }

    Err(DebError::ControlFileNotFound)
}
