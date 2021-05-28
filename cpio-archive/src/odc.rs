// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Portable ASCII format / old character / odc archive support.
//!
//! This module implements support for the *Portable ASCII format* as
//! standardized in version 2 of the Single UNIX Specification (SUSv2).
//! It is also commonly referred to as *old character* or *odc*.

use {
    crate::{CpioHeader, CpioReader, CpioResult, Error},
    std::{
        ffi::CStr,
        io::{Read, Take},
    },
};

/// Header magic for odc entries.
pub const MAGIC: &[u8] = b"070707";

fn u32_from_octal(data: &[u8]) -> CpioResult<u32> {
    let s = std::str::from_utf8(data).map_err(|_| Error::BadHeaderString)?;
    u32::from_str_radix(s, 8).map_err(|_| Error::BadHeaderHex(s.to_string()))
}

fn read_octal(reader: &mut impl Read, count: usize) -> CpioResult<u32> {
    let mut buffer = vec![0u8; count];
    reader.read_exact(&mut buffer)?;

    u32_from_octal(&buffer)
}

/// Parsed portable ASCII format header.
#[derive(Clone, Debug)]
pub struct OdcHeader {
    pub dev: u32,
    pub inode: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u32,
    pub rdev: u32,
    pub mtime: u32,
    pub file_size: u32,
    pub name: String,
}

impl OdcHeader {
    /// Parse a header from a reader.
    pub fn from_reader(reader: &mut impl Read) -> CpioResult<Self> {
        let dev = read_octal(reader, 6)?;
        let inode = read_octal(reader, 6)?;
        let mode = read_octal(reader, 6)?;
        let uid = read_octal(reader, 6)?;
        let gid = read_octal(reader, 6)?;
        let nlink = read_octal(reader, 6)?;
        let rdev = read_octal(reader, 6)?;
        let mtime = read_octal(reader, 11)?;
        let name_length = read_octal(reader, 6)?;
        let file_size = read_octal(reader, 11)?;

        let mut name_data = vec![0u8; name_length as usize];
        reader.read_exact(&mut name_data)?;

        let name = CStr::from_bytes_with_nul(&name_data)
            .map_err(|_| Error::FilenameDecode)?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            dev,
            inode,
            mode,
            uid,
            gid,
            nlink,
            rdev,
            mtime,
            file_size,
            name,
        })
    }
}

impl CpioHeader for OdcHeader {
    fn device(&self) -> u32 {
        self.dev
    }

    fn inode(&self) -> u32 {
        self.inode
    }

    fn mode(&self) -> u32 {
        self.mode
    }

    fn uid(&self) -> u32 {
        self.uid
    }

    fn gid(&self) -> u32 {
        self.gid
    }

    fn nlink(&self) -> u32 {
        self.nlink
    }

    fn rdev(&self) -> u32 {
        self.rdev
    }

    fn mtime(&self) -> u32 {
        self.mtime
    }

    fn file_size(&self) -> u32 {
        self.file_size
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A cpio archive reader for *Portable ASCII format* archives.
pub struct OdcReader<T: Read + Sized> {
    archive_reader: Option<T>,
    entry_reader: Option<Take<T>>,
    seen_trailer: bool,
}

impl<T: Read + Sized> CpioReader<T> for OdcReader<T> {
    fn new(reader: T) -> Self {
        Self {
            archive_reader: Some(reader),
            entry_reader: None,
            seen_trailer: false,
        }
    }

    fn read_next(&mut self) -> CpioResult<Option<Box<dyn CpioHeader>>> {
        self.finish()?;

        if let Some(mut reader) = self.archive_reader.take() {
            let mut magic = [0u8; 6];

            match reader.read_exact(&mut magic) {
                Ok(_) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(None);
                }
                Err(e) => {
                    return Err(e.into());
                }
            }

            if magic != MAGIC {
                return Err(Error::BadMagic);
            }

            let header = OdcHeader::from_reader(&mut reader)?;

            if header.name == "TRAILER!!!" {
                self.seen_trailer = true;
                Ok(None)
            } else {
                self.entry_reader = Some(reader.take(header.file_size as _));
                Ok(Some(Box::new(header)))
            }
        } else {
            Ok(None)
        }
    }

    fn finish(&mut self) -> CpioResult<()> {
        if let Some(mut reader) = self.entry_reader.take() {
            let mut buffer = vec![0u8; 32768];
            loop {
                if reader.read(&mut buffer)? == 0 {
                    break;
                }
            }

            // Only restore the archive reader if we haven't seen the trailer,
            // as the trailer indicates end of archive.
            if !self.seen_trailer {
                self.archive_reader = Some(reader.into_inner());
            }
        }

        Ok(())
    }
}

impl<T: Read + Sized> Iterator for OdcReader<T> {
    type Item = CpioResult<Box<dyn CpioHeader>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_next() {
            Ok(Some(r)) => Some(Ok(r)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

impl<T: Read + Sized> Read for OdcReader<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        if let Some(reader) = &mut self.entry_reader {
            reader.read(buf)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "no current archive entry to read from",
            ))
        }
    }
}
