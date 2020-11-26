// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Interfaces for .deb package files.

The .deb file specification lives at https://manpages.debian.org/unstable/dpkg-dev/deb.5.en.html.
*/

use {
    crate::debian::ControlFile,
    os_str_bytes::OsStrBytes,
    std::{
        io::{BufWriter, Cursor, Read, Write},
        path::Path,
        time::SystemTime,
    },
    virtual_file_manifest::FileManifest,
};

/// Represents an error related to .deb file handling.
#[derive(Debug)]
pub enum DebError {
    IOError(std::io::Error),
    PathError(String),
}

impl std::fmt::Display for DebError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IOError(inner) => write!(f, "I/O error: {}", inner),
            Self::PathError(msg) => write!(f, "path error: {}", msg),
        }
    }
}

impl std::error::Error for DebError {}

impl From<std::io::Error> for DebError {
    fn from(e: std::io::Error) -> Self {
        Self::IOError(e)
    }
}

impl<W> From<std::io::IntoInnerError<W>> for DebError {
    fn from(e: std::io::IntoInnerError<W>) -> Self {
        Self::IOError(e.into())
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

/// A builder for a `.deb` package file.
pub struct DebBuilder<'control> {
    control_file: ControlFile<'control>,

    compression: DebCompression,

    /// Files to install as part of the package.
    install_files: FileManifest,

    mtime: Option<SystemTime>,
}

impl<'control> DebBuilder<'control> {
    /// Construct a new instance using a control file.
    pub fn new(control_file: ControlFile<'control>) -> Self {
        Self {
            control_file,
            compression: DebCompression::Gzip,
            install_files: FileManifest::default(),
            mtime: None,
        }
    }

    /// Set the compression format to use.
    ///
    /// Note all compression formats are supported by all Linux distributions.
    pub fn set_compression(mut self, compression: DebCompression) -> Self {
        self.compression = compression;
        self
    }

    fn mtime(&self) -> u64 {
        self.mtime
            .unwrap_or_else(std::time::SystemTime::now)
            .duration_since(std::time::UNIX_EPOCH)
            .expect("times before UNIX epoch not accepted")
            .as_secs()
    }

    pub fn set_mtime(mut self, time: Option<SystemTime>) -> Self {
        self.mtime = time;
        self
    }

    /// Write `.deb` file content to a writer.
    pub fn write<W: Write>(&self, writer: &mut W) -> Result<(), DebError> {
        let mut ar_builder = ar::Builder::new(writer);

        // First entry is a debian-binary file with static content.
        let data: &[u8] = b"2.0\n";
        let mut header = ar::Header::new(b"debian-binary".to_vec(), data.len() as _);
        header.set_mode(0o644);
        header.set_mtime(self.mtime());
        header.set_uid(0);
        header.set_gid(0);
        ar_builder.append(&header, data)?;

        // Second entry is a control.tar with metadata.
        let mut control_builder =
            ControlTarBuilder::new(self.control_file.clone()).set_mtime(self.mtime);

        for (rel_path, content) in self.install_files.iter_entries() {
            let data = content.data.resolve()?;
            let mut cursor = Cursor::new(&data);
            control_builder = control_builder.add_file(rel_path, &mut cursor)?;
        }

        let mut control_writer = BufWriter::new(Vec::new());
        control_builder.write(&mut control_writer)?;
        let control_tar = control_writer.into_inner()?;
        let control_tar = self
            .compression
            .compress(&mut std::io::Cursor::new(control_tar))?;

        let mut header = ar::Header::new(
            format!("control.tar{}", self.compression.extension()).into_bytes(),
            control_tar.len() as _,
        );
        header.set_mode(0o644);
        header.set_mtime(self.mtime());
        header.set_uid(0);
        header.set_gid(0);
        ar_builder.append(&header, &*control_tar)?;

        // Third entry is a data.tar with file content.
        let mut data_writer = BufWriter::new(Vec::new());
        write_data_tar(&mut data_writer, &self.install_files, self.mtime())?;
        let data_tar = data_writer.into_inner()?;
        let data_tar = self
            .compression
            .compress(&mut std::io::Cursor::new(data_tar))?;

        let mut header = ar::Header::new(
            format!("data.tar{}", self.compression.extension()).into_bytes(),
            data_tar.len() as _,
        );
        header.set_mode(0o644);
        header.set_mtime(self.mtime());
        header.set_uid(0);
        header.set_gid(0);
        ar_builder.append(&header, &*data_tar)?;

        Ok(())
    }
}

fn new_tar_header(mtime: u64) -> Result<tar::Header, DebError> {
    let mut header = tar::Header::new_gnu();
    header.set_uid(0);
    header.set_gid(0);
    header.set_username("root")?;
    header.set_groupname("root")?;
    header.set_mtime(mtime);

    Ok(header)
}

fn set_header_path(
    builder: &mut tar::Builder<impl Write>,
    header: &mut tar::Header,
    path: &Path,
    is_directory: bool,
) -> Result<(), DebError> {
    // Debian archives in the wild have filenames beginning with `./`. And
    // paths ending with `/` are directories. However, we cannot call
    // `header.set_path()` with `./` on anything except the root directory
    // because it will normalize away the `./` bit. So we set the header field
    // directly when adding directories and files.

    // We should only be dealing with GNU headers, which simplifies our code a bit.
    assert!(header.as_ustar().is_none());

    let value = format!(
        "./{}{}",
        path.display(),
        if is_directory { "/" } else { "" }
    );
    let value_bytes = value.as_bytes();

    let name_buffer = &mut header.as_old_mut().name;

    // If it fits within the buffer, copy it over.
    if value_bytes.len() <= name_buffer.len() {
        name_buffer[0..value_bytes.len()].copy_from_slice(value_bytes);
    } else {
        // Else we emit a special entry to extend the filename. Who knew tar
        // files were this jank.
        let mut header2 = tar::Header::new_gnu();
        let name = b"././@LongLink";
        header2.as_gnu_mut().unwrap().name[..name.len()].clone_from_slice(&name[..]);
        header2.set_mode(0o644);
        header2.set_uid(0);
        header2.set_gid(0);
        header2.set_mtime(0);
        header2.set_size(value_bytes.len() as u64 + 1);
        header2.set_entry_type(tar::EntryType::new(b'L'));
        header2.set_cksum();
        let mut data = value_bytes.chain(std::io::repeat(0).take(1));
        builder.append(&header2, &mut data)?;

        let truncated_bytes = &value_bytes[0..name_buffer.len()];
        name_buffer[0..truncated_bytes.len()].copy_from_slice(truncated_bytes);
    }

    Ok(())
}

/// A builder for a `control.tar` file inside `.deb` packages.
pub struct ControlTarBuilder<'a> {
    /// The file that will become the `control` file.
    control: ControlFile<'a>,
    md5sums: Vec<Vec<u8>>,
    mtime: Option<SystemTime>,
}

impl<'a> ControlTarBuilder<'a> {
    /// Create a new instance from a control file.
    pub fn new(control_file: ControlFile<'a>) -> Self {
        Self {
            control: control_file,
            md5sums: vec![],
            mtime: None,
        }
    }

    /// Add a file to be indexed.
    ///
    /// `path` is the relative path the file will be installed to.
    /// `reader` is a reader to obtain the file content.
    ///
    /// This method has the side-effect of computing the checksum for the path
    /// so a checksums entry can be written.
    pub fn add_file<P: AsRef<Path>, R: Read>(
        mut self,
        path: P,
        reader: &mut R,
    ) -> Result<Self, DebError> {
        let mut context = md5::Context::new();

        let mut buffer = [0; 32768];

        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }

            context.consume(&buffer[0..read]);
        }

        let digest = context.compute();

        let mut entry = Vec::new();
        entry.write_all(&digest.to_ascii_lowercase())?;
        entry.write_all(b"  ")?;
        entry.write_all(path.as_ref().to_bytes().as_ref())?;
        entry.write_all(b"\n")?;

        self.md5sums.push(entry);

        Ok(self)
    }

    fn mtime(&self) -> u64 {
        self.mtime
            .unwrap_or_else(std::time::SystemTime::now)
            .duration_since(std::time::UNIX_EPOCH)
            .expect("times before UNIX epoch not accepted")
            .as_secs()
    }

    pub fn set_mtime(mut self, time: Option<SystemTime>) -> Self {
        self.mtime = time;
        self
    }

    /// Write the `control.tar` file to a writer.
    pub fn write<W: Write>(&self, writer: &mut W) -> Result<(), DebError> {
        let mut control_buffer = BufWriter::new(Vec::new());
        self.control.write(&mut control_buffer)?;
        let control_data = control_buffer.into_inner()?;

        let mut builder = tar::Builder::new(writer);

        let mut header = new_tar_header(self.mtime())?;
        header.set_path("control")?;
        header.set_mode(0o644);
        header.set_size(control_data.len() as _);
        header.set_cksum();
        builder.append(&header, &*control_data)?;

        // Write the md5sums file.
        let md5sums = self.md5sums.concat::<u8>();
        let mut header = new_tar_header(self.mtime())?;
        header.set_path("md5sums")?;
        header.set_mode(0o644);
        header.set_size(md5sums.len() as _);
        header.set_cksum();
        builder.append(&header, &*md5sums)?;

        // We could also support maintainer scripts. For another day...

        builder.finish()?;

        Ok(())
    }
}

/// Write `data.tar` content to a writer.
pub fn write_data_tar<W: Write>(
    writer: W,
    files: &FileManifest,
    mtime: u64,
) -> Result<(), DebError> {
    let mut builder = tar::Builder::new(writer);

    // Add root directory entry.
    let mut header = new_tar_header(mtime)?;
    header.set_path(Path::new("./"))?;
    header.set_mode(0o755);
    header.set_size(0);
    header.set_cksum();
    builder.append(&header, &*vec![])?;

    // And entries for each directory in the tree.
    for directory in files.relative_directories() {
        let mut header = new_tar_header(mtime)?;
        set_header_path(&mut builder, &mut header, &directory, true)?;
        header.set_mode(0o755);
        header.set_size(0);
        header.set_cksum();
        builder.append(&header, &*vec![])?;
    }

    // FileManifest is backed by a BTreeMap, so iteration is deterministic.
    for (rel_path, content) in files.iter_entries() {
        let data = content.data.resolve()?;

        let mut header = new_tar_header(mtime)?;
        set_header_path(&mut builder, &mut header, rel_path, false)?;
        header.set_mode(if content.executable { 0o755 } else { 0o644 });
        header.set_size(data.len() as _);
        header.set_cksum();
        builder.append(&header, &*data)?;
    }

    builder.finish()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        anyhow::{anyhow, Result},
        std::path::PathBuf,
        virtual_file_manifest::FileEntry,
    };

    #[test]
    fn test_write_data_tar_one_file() -> Result<()> {
        let mut manifest = FileManifest::default();
        manifest.add_file_entry(
            "foo/bar.txt",
            FileEntry {
                data: vec![42].into(),
                executable: true,
            },
        )?;

        let mut buffer = vec![];
        write_data_tar(&mut buffer, &manifest, 2)?;

        let mut archive = tar::Archive::new(std::io::Cursor::new(buffer));

        for (i, entry) in archive.entries()?.enumerate() {
            let entry = entry?;

            let path = match i {
                0 => Path::new("./"),
                1 => Path::new("./foo/"),
                2 => Path::new("./foo/bar.txt"),
                _ => return Err(anyhow!("unexpected archive entry")),
            };

            assert_eq!(entry.path()?, path, "entry {} path matches", i);
        }

        Ok(())
    }

    #[test]
    fn test_write_data_tar_long_path() -> Result<()> {
        let long_path = PathBuf::from(format!("f{}.txt", "u".repeat(200)));

        let mut manifest = FileManifest::default();

        manifest.add_file_entry(
            &long_path,
            FileEntry {
                data: vec![42].into(),
                executable: false,
            },
        )?;

        let mut buffer = vec![];
        write_data_tar(&mut buffer, &manifest, 2)?;

        let mut archive = tar::Archive::new(std::io::Cursor::new(buffer));

        for (i, entry) in archive.entries()?.enumerate() {
            let entry = entry?;

            if i != 1 {
                continue;
            }

            assert_eq!(
                entry.path()?,
                Path::new(&format!("./f{}.txt", "u".repeat(200)))
            );
        }

        Ok(())
    }
}
