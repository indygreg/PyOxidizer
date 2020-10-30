// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Interfaces for .deb package files.

The .deb file specification lives at https://manpages.debian.org/unstable/dpkg-dev/deb.5.en.html.
*/

use {
    crate::{debian::ControlFile, file_resource::FileManifest},
    os_str_bytes::OsStrBytes,
    std::{
        io::{BufWriter, Cursor, Read, Write},
        path::Path,
        time::SystemTime,
    },
};

/// Represents an error related to .deb file handling.
pub enum DebError {
    IOError(std::io::Error),
}

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

/// A builder for a `.deb` package file.
pub struct DebBuilder<'a> {
    control_file: ControlFile<'a>,

    /// Files to install as part of the package.
    install_files: FileManifest,

    mtime: Option<SystemTime>,
}

impl<'a> DebBuilder<'a> {
    /// Construct a new instance using a control file.
    pub fn new(control_file: ControlFile<'a>) -> Self {
        Self {
            control_file,
            install_files: FileManifest::default(),
            mtime: None,
        }
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
        let mut header = ar::Header::new(b"debian-history".to_vec(), data.len() as _);
        header.set_mode(0o644);
        header.set_mtime(self.mtime());
        header.set_uid(0);
        header.set_gid(0);
        ar_builder.append(&header, data)?;

        // Second entry is a control.tar with metadata.
        let mut control_builder =
            ControlTarBuilder::new(self.control_file.clone()).set_mtime(self.mtime.clone());

        for (rel_path, content) in self.install_files.entries() {
            let mut cursor = Cursor::new(&content.data);
            control_builder = control_builder.add_file(rel_path, &mut cursor)?;
        }

        let mut control_writer = BufWriter::new(Vec::new());
        control_builder.write(&mut control_writer)?;
        let control_tar = control_writer.into_inner()?;

        let mut header = ar::Header::new(b"control.tar".to_vec(), control_tar.len() as _);
        header.set_mode(0o644);
        header.set_mtime(self.mtime());
        header.set_uid(0);
        header.set_gid(0);
        ar_builder.append(&header, &*control_tar)?;

        // Third entry is a data.tar with file content.
        let mut data_writer = BufWriter::new(Vec::new());
        write_data_tar(&mut data_writer, &self.install_files, self.mtime())?;
        let data_tar = data_writer.into_inner()?;
        // TODO compress data

        let mut header = ar::Header::new(b"data.tar".to_vec(), data_tar.len() as _);
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
        header.set_path(Path::new("./").join(directory))?;
        header.set_mode(0o755);
        header.set_size(0);
        header.set_cksum();
        builder.append(&header, &*vec![])?;
    }

    // FileManifest is backed by a BTreeMap, so iteration is deterministic.
    for (rel_path, content) in files.entries() {
        let mut header = new_tar_header(mtime)?;
        header.set_path(Path::new("./").join(rel_path))?;
        header.set_mode(if content.executable { 0o755 } else { 0o644 });
        header.set_size(content.data.len() as _);
        header.set_cksum();
        builder.append(&header, &*content.data)?;
    }

    builder.finish()?;

    Ok(())
}
