// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! New ASCII format support.

use {
    crate::{CpioHeader, CpioReader, CpioResult, Error},
    std::{
        ffi::CStr,
        io::{Read, Take},
    },
};

pub const MAGIC: &[u8] = b"070701";

fn u32_from_hex(data: &[u8]) -> CpioResult<u32> {
    let s = std::str::from_utf8(data).map_err(|_| Error::BadHeaderString)?;
    u32::from_str_radix(s, 16).map_err(|_| Error::BadHeaderHex(s.to_string()))
}

fn read_hex(reader: &mut impl Read, count: usize) -> CpioResult<u32> {
    let mut buffer = vec![0u8; count];
    reader.read_exact(&mut buffer)?;

    u32_from_hex(&buffer)
}

#[derive(Clone, Debug)]
pub struct NewcHeader {
    pub inode: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u32,
    pub mtime: u32,
    pub file_size: u32,
    pub dev_major: u32,
    pub dev_minor: u32,
    pub rdev_major: u32,
    pub rdev_minor: u32,
    pub checksum: u32,
    pub name: String,
}

impl NewcHeader {
    pub fn from_reader(reader: &mut impl Read) -> CpioResult<Self> {
        let inode = read_hex(reader, 8)?;
        let mode = read_hex(reader, 8)?;
        let uid = read_hex(reader, 8)?;
        let gid = read_hex(reader, 8)?;
        let nlink = read_hex(reader, 8)?;
        let mtime = read_hex(reader, 8)?;
        let file_size = read_hex(reader, 8)?;
        let dev_major = read_hex(reader, 8)?;
        let dev_minor = read_hex(reader, 8)?;
        let rdev_major = read_hex(reader, 8)?;
        let rdev_minor = read_hex(reader, 8)?;
        let name_length = read_hex(reader, 8)?;
        let checksum = read_hex(reader, 8)?;

        let mut name_data = vec![0u8; name_length as usize];
        reader.read_exact(&mut name_data)?;

        let name = CStr::from_bytes_with_nul(&name_data)
            .map_err(|_| Error::FilenameDecode)?
            .to_string_lossy()
            .to_string();

        // Pad to 4 byte boundary.
        let mut pad = vec![0u8; name_data.len() % 4];
        reader.read_exact(&mut pad)?;

        Ok(Self {
            inode,
            mode,
            uid,
            gid,
            nlink,
            mtime,
            file_size,
            dev_major,
            dev_minor,
            rdev_major,
            rdev_minor,
            checksum,
            name,
        })
    }
}

impl CpioHeader for NewcHeader {
    fn device(&self) -> u32 {
        todo!()
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
        todo!()
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

/// A cpio archive reader for *New ASCII format* archives.
pub struct NewcReader<T: Read + Sized> {
    archive_reader: Option<T>,
    entry_reader: Option<Take<T>>,
    entry_data_pad: usize,
    seen_trailer: bool,
}

impl<T: Read + Sized> CpioReader<T> for NewcReader<T> {
    fn new(reader: T) -> Self {
        Self {
            archive_reader: Some(reader),
            entry_reader: None,
            entry_data_pad: 0,
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

            let header = NewcHeader::from_reader(&mut reader)?;

            if header.name == "TRAILER!!!" {
                self.seen_trailer = true;
                Ok(None)
            } else {
                self.entry_reader = Some(reader.take(header.file_size as _));
                self.entry_data_pad = header.file_size as usize % 4;
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

            let mut reader = reader.into_inner();

            let mut pad = vec![0u8; self.entry_data_pad];
            reader.read_exact(&mut pad)?;
            self.entry_data_pad = 0;

            // Only restore the archive reader if we haven't seen the trailer,
            // as the trailer indicates end of archive.
            if !self.seen_trailer {
                self.archive_reader = Some(reader);
            }
        }

        Ok(())
    }
}

impl<T: Read + Sized> Iterator for NewcReader<T> {
    type Item = CpioResult<Box<dyn CpioHeader>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_next() {
            Ok(Some(r)) => Some(Ok(r)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

impl<T: Read + Sized> Read for NewcReader<T> {
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
