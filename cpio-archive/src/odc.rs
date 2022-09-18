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
    chrono::{DateTime, Utc},
    is_executable::IsExecutable,
    simple_file_manifest::{
        FileManifest, S_IFDIR, S_IRGRP, S_IROTH, S_IRUSR, S_IWUSR, S_IXGRP, S_IXOTH, S_IXUSR,
    },
    std::{
        collections::HashSet,
        ffi::CStr,
        io::{Read, Take, Write},
        path::Path,
    },
};

/// Header magic for odc entries.
pub const MAGIC: &[u8] = b"070707";

const TRAILER: &str = "TRAILER!!!";

fn u32_from_octal(data: &[u8]) -> CpioResult<u32> {
    let s = std::str::from_utf8(data).map_err(|_| Error::BadHeaderString)?;
    u32::from_str_radix(s, 8).map_err(|_| Error::BadHeaderHex(s.to_string()))
}

fn u64_from_octal(data: &[u8]) -> CpioResult<u64> {
    let s = std::str::from_utf8(data).map_err(|_| Error::BadHeaderString)?;
    u64::from_str_radix(s, 8).map_err(|_| Error::BadHeaderHex(s.to_string()))
}

fn read_octal_u32(reader: &mut impl Read, count: usize) -> CpioResult<u32> {
    let mut buffer = vec![0u8; count];
    reader.read_exact(&mut buffer)?;

    u32_from_octal(&buffer)
}

fn read_octal_u64(reader: &mut impl Read, count: usize) -> CpioResult<u64> {
    let mut buffer = vec![0u8; count];
    reader.read_exact(&mut buffer)?;

    u64_from_octal(&buffer)
}

fn write_octal(value: u64, writer: &mut impl Write, size: usize) -> CpioResult<()> {
    let max_value = 8u64.pow(size as _);

    if value > max_value {
        return Err(Error::ValueTooLarge);
    }

    let s = format!("{:o}", value);

    for _ in 0..size - s.len() {
        writer.write_all(b"0")?;
    }

    writer.write_all(s.as_bytes())?;

    Ok(())
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
    pub file_size: u64,
    pub name: String,
}

impl OdcHeader {
    /// Parse a header from a reader.
    pub fn from_reader(reader: &mut impl Read) -> CpioResult<Self> {
        let dev = read_octal_u32(reader, 6)?;
        let inode = read_octal_u32(reader, 6)?;
        let mode = read_octal_u32(reader, 6)?;
        let uid = read_octal_u32(reader, 6)?;
        let gid = read_octal_u32(reader, 6)?;
        let nlink = read_octal_u32(reader, 6)?;
        let rdev = read_octal_u32(reader, 6)?;
        let mtime = read_octal_u32(reader, 11)?;
        let name_length = read_octal_u32(reader, 6)?;
        let file_size = read_octal_u64(reader, 11)?;

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

    /// Write the binary header content to a writer.
    pub fn write(&self, writer: &mut impl Write) -> CpioResult<u64> {
        writer.write_all(MAGIC)?;
        write_octal(self.dev as _, writer, 6)?;
        write_octal(self.inode as _, writer, 6)?;
        write_octal(self.mode as _, writer, 6)?;
        write_octal(self.uid as _, writer, 6)?;
        write_octal(self.gid as _, writer, 6)?;
        write_octal(self.nlink as _, writer, 6)?;
        write_octal(self.rdev as _, writer, 6)?;
        write_octal(self.mtime as _, writer, 11)?;
        write_octal(self.name.len() as u64 + 1u64, writer, 6)?;
        write_octal(self.file_size, writer, 11)?;

        writer.write_all(self.name.as_bytes())?;
        writer.write_all(b"\0")?;

        Ok(9 * 6 + 11 * 2 + self.name.len() as u64 + 1)
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

    fn file_size(&self) -> u64 {
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

            if header.name == TRAILER {
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

/// Iteratively create a cpio archive using the *Portable ASCII format*.
///
/// cpio archives logically consist of 2-tuples of (file header, data), so
/// data can be streamed by iteratively feeding new entries to write.
///
/// cpio archives contain a special file header denoting the end of the
/// archive. This is emitted by calling [Self::finish]. So consumers should
/// always call this method when done writing new files.
///
/// By default, missing parent directories are automatically emitted when
/// writing files. Instances track which directories have been emitted. Upon
/// encountering a file path in a directory that has not yet been emitted,
/// a directory entry will be emitted. This behavior can be disabled by
/// calling [Self::auto_write_dirs].
pub struct OdcBuilder<W: Write + Sized> {
    writer: W,
    default_uid: u32,
    default_gid: u32,
    default_mtime: DateTime<Utc>,
    default_mode_file: u32,
    default_mode_dir: u32,
    auto_write_dirs: bool,
    seen_dirs: HashSet<String>,
    entry_count: u32,
    finished: bool,
}

impl<W: Write + Sized> OdcBuilder<W> {
    /// Construct a new instance which will write data to a writer.
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            default_uid: 0,
            default_gid: 0,
            default_mtime: Utc::now(),
            default_mode_file: S_IRUSR | S_IWUSR | S_IRGRP | S_IROTH,
            default_mode_dir: S_IFDIR
                | S_IRUSR
                | S_IWUSR
                | S_IXUSR
                | S_IRGRP
                | S_IXGRP
                | S_IROTH
                | S_IXOTH,
            auto_write_dirs: true,
            seen_dirs: HashSet::new(),
            entry_count: 0,
            finished: false,
        }
    }

    /// Set the default file mode to use for files.
    pub fn default_mode_file(&mut self, mode: u32) {
        self.default_mode_file = mode;
    }

    /// Set the default file mode to use for directories.
    pub fn default_mode_directory(&mut self, mode: u32) {
        self.default_mode_dir = mode;
    }

    /// Set the default user ID (UID).
    pub fn default_user_id(&mut self, uid: u32) {
        self.default_uid = uid;
    }

    /// Set the default group ID (GID).
    pub fn default_group_id(&mut self, gid: u32) {
        self.default_gid = gid;
    }

    /// Set the default modified time.
    pub fn default_mtime(&mut self, mtime: DateTime<Utc>) {
        self.default_mtime = mtime;
    }

    /// Set the behavior for auto writing directory entries.
    pub fn auto_write_dirs(&mut self, value: bool) {
        self.auto_write_dirs = value;
    }

    /// Obtain a header record representing the next header in the archive.
    ///
    /// The header has fields set to default values. Callers should likely
    /// update at least the name and possibly the file size and mode.
    ///
    /// This will increment the inode sequence number when called.
    pub fn next_header(&mut self) -> OdcHeader {
        let inode = self.entry_count;
        self.entry_count += 1;

        OdcHeader {
            dev: 0,
            inode,
            mode: self.default_mode_file,
            uid: self.default_uid,
            gid: self.default_gid,
            nlink: 0,
            rdev: 0,
            mtime: self.default_mtime.timestamp() as _,
            file_size: 0,
            name: "".to_string(),
        }
    }

    fn normalize_archive_path(&self, path: &str) -> String {
        if path.starts_with("./") {
            path.to_string()
        } else {
            format!("./{}", path)
        }
    }

    /// Write missing parent directory entries for a given file path.
    fn emit_parent_directories(&mut self, file_path: &str) -> CpioResult<u64> {
        let parts = file_path.split('/').collect::<Vec<_>>();

        let mut bytes_written = 0;

        for idx in 1..parts.len() {
            let dir = parts
                .clone()
                .into_iter()
                .take(idx)
                .collect::<Vec<&str>>()
                .join("/");

            if !self.seen_dirs.contains(&dir) {
                let mut header = self.next_header();
                header.mode = self.default_mode_dir;
                header.name = dir.clone();

                bytes_written += header.write(&mut self.writer)?;
                self.seen_dirs.insert(dir);
            }
        }

        Ok(bytes_written)
    }

    /// Append a raw header and corresponding file data to the writer.
    ///
    /// The writer and data are written as-is.
    ///
    /// Only simple validation that the data length matches the length advertised
    /// in the header is performed.
    ///
    /// Automatic directory emission is not processed in this mode.
    pub fn append_header_with_data(
        &mut self,
        header: OdcHeader,
        data: impl AsRef<[u8]>,
    ) -> CpioResult<u64> {
        let data = data.as_ref();

        if header.file_size as usize != data.len() {
            return Err(Error::SizeMismatch);
        }

        let written = header.write(&mut self.writer)?;
        self.writer.write_all(data)?;

        Ok(written + data.len() as u64)
    }

    /// Append a raw header and corresponding data from a reader to the writer.
    ///
    /// The header's file size must match the length of data available in the reader
    /// or errors could occur. This method will copy all data available in the reader
    /// to the output stream. If the number of bytes written does not match what is
    /// reported by the header, the cpio archive stream is effectively corrupted
    /// and an error is returned.
    pub fn append_header_with_reader(
        &mut self,
        header: OdcHeader,
        reader: &mut impl Read,
    ) -> CpioResult<u64> {
        let written = header.write(&mut self.writer)?;
        let copied = std::io::copy(reader, &mut self.writer)?;

        if copied != header.file_size {
            Err(Error::SizeMismatch)
        } else {
            Ok(written + copied)
        }
    }

    /// Write a regular file to the cpio archive with provided file data and file mode.
    pub fn append_file_from_data(
        &mut self,
        archive_path: impl ToString,
        data: impl AsRef<[u8]>,
        mode: u32,
    ) -> CpioResult<u64> {
        let archive_path = self.normalize_archive_path(&archive_path.to_string());
        let data = data.as_ref();

        let mut bytes_written = self.emit_parent_directories(&archive_path)?;

        let mut header = self.next_header();
        header.name = archive_path;
        header.file_size = data.len() as _;
        header.mode = mode;

        bytes_written += header.write(&mut self.writer)?;
        self.writer.write_all(data)?;
        bytes_written += data.len() as u64;

        Ok(bytes_written)
    }

    /// Write a regular file to the cpio archive.
    ///
    /// This takes the relative path in the archive and the filesystem path of
    /// the file to write. It resolves header metadata automatically given filesystem
    /// attributes. However, the UID, GID, and mtime defaults specified on this
    /// builder are used so archive construction is more deterministic.
    pub fn append_file_from_path(
        &mut self,
        archive_path: impl ToString,
        path: impl AsRef<Path>,
    ) -> CpioResult<u64> {
        let archive_path = self.normalize_archive_path(&archive_path.to_string());
        let path = path.as_ref();

        let mut fh = std::fs::File::open(path)?;
        let metadata = fh.metadata()?;

        if !metadata.is_file() {
            return Err(Error::NotAFile(path.to_path_buf()));
        }

        // Emit parent directories first, so inode number is sequential.
        let mut bytes_written = self.emit_parent_directories(&archive_path)?;

        let mut header = self.next_header();
        header.name = archive_path;
        header.file_size = metadata.len();

        if path.is_executable() {
            header.mode |= S_IXUSR | S_IXGRP | S_IXOTH;
        }

        bytes_written += header.write(&mut self.writer)?;
        bytes_written += std::io::copy(&mut fh, &mut self.writer)?;

        Ok(bytes_written)
    }

    /// Append a [FileManifest] to the archive.
    pub fn append_file_manifest(&mut self, manifest: &FileManifest) -> CpioResult<u64> {
        let mut bytes_written = 0;

        for (path, entry) in manifest.iter_entries() {
            let mode = if entry.is_executable() { 0o755 } else { 0o644 };
            let data = entry.resolve_content()?;

            bytes_written += self.append_file_from_data(path.display().to_string(), data, mode)?;
        }

        Ok(bytes_written)
    }

    /// Finish writing the archive.
    ///
    /// This will emit a special header denoting the end of archive.
    ///
    /// Failure to call this method will result in a malformed cpio archive.
    /// Readers may or may not handle the missing trailer correctly.
    pub fn finish(&mut self) -> CpioResult<u64> {
        if !self.finished {
            let mut header = self.next_header();
            header.name = TRAILER.to_string();
            let count = header.write(&mut self.writer)?;
            self.finished = true;

            Ok(count)
        } else {
            Ok(0)
        }
    }

    /// Consume self and return the original writer this instance was constructed from.
    ///
    /// This will automatically finish the archive if needed.
    pub fn into_inner(mut self) -> CpioResult<W> {
        self.finish()?;

        Ok(self.writer)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, std::io::Cursor};

    #[test]
    fn write_single_file() {
        let mut builder = OdcBuilder::new(Cursor::new(Vec::<u8>::new()));

        let current_exe = std::env::current_exe().unwrap();
        let current_exe_data = std::fs::read(&current_exe).unwrap();
        builder
            .append_file_from_path("child/grandchild/exe", current_exe)
            .unwrap();

        let mut reader = builder.into_inner().unwrap();
        reader.set_position(0);

        let mut reader = OdcReader::new(reader);

        let mut i = 0;
        while let Some(header) = reader.read_next().unwrap() {
            let mut file_data = Vec::<u8>::with_capacity(header.file_size() as _);
            reader.read_to_end(&mut file_data).unwrap();

            let wanted_filename = match i {
                0 => ".",
                1 => "./child",
                2 => "./child/grandchild",
                3 => "./child/grandchild/exe",
                _ => panic!("unexpected entry in archive: {:?}", header),
            };

            assert_eq!(header.name(), wanted_filename);

            if (0..=2).contains(&i) {
                assert_eq!(header.file_size(), 0);
                assert_ne!(header.mode() & S_IFDIR, 0);
            }

            if i == 3 {
                assert_eq!(&file_data, &current_exe_data);
            }

            i += 1;
        }
    }
}
