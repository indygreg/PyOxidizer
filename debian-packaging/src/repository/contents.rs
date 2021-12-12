// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! `Contents` index file handling. */

use {
    crate::error::Result,
    futures::{AsyncBufRead, AsyncBufReadExt},
    pin_project::pin_project,
    std::{
        collections::{BTreeMap, BTreeSet},
        io::{BufRead, Write},
    },
};

/// Represents a `Contents` file.
///
/// A `Contents` file maps paths to packages. It facilitates lookups of which paths
/// are in which packages.
///
/// Internally, paths are stored as [String] because bulk operations against paths
/// can be expensive due to more expensive comparison/equality checks.
#[derive(Clone, Debug, Default)]
pub struct ContentsFile {
    /// Mapping of paths to packages they occur in.
    paths: BTreeMap<String, BTreeSet<String>>,
    /// Mapping of package names to paths they contain.
    packages: BTreeMap<String, BTreeSet<String>>,
}

impl ContentsFile {
    fn parse_and_add_line(&mut self, line: &str) -> Result<()> {
        // According to https://wiki.debian.org/DebianRepository/Format#A.22Contents.22_indices
        // `Contents` files begin with freeform text then have a table of path to package list.
        // Invalid lines are ignored.

        let words = line.split_ascii_whitespace().collect::<Vec<_>>();

        if words.len() != 2 {
            return Ok(());
        }

        let path = words[0];
        let packages = words[1];

        for package in packages.split(',') {
            self.paths
                .entry(path.to_string())
                .or_default()
                .insert(package.to_string());
            self.packages
                .entry(package.to_string())
                .or_default()
                .insert(path.to_string());
        }

        Ok(())
    }

    /// Register a path as belonging to a package.
    pub fn add_package_path(&mut self, path: String, package: String) {
        self.paths
            .entry(path.clone())
            .or_default()
            .insert(package.clone());
        self.packages.entry(package).or_default().insert(path);
    }

    /// Obtain an iterator of packages having the specified path.
    pub fn packages_with_path(&self, path: &str) -> Box<dyn Iterator<Item = &str> + '_> {
        if let Some(packages) = self.paths.get(path) {
            Box::new(packages.iter().map(|x| x.as_str()))
        } else {
            Box::new(std::iter::empty())
        }
    }

    /// Obtain an iterator of paths in a given package.
    pub fn package_paths(&self, package: &str) -> Box<dyn Iterator<Item = &str> + '_> {
        if let Some(paths) = self.packages.get(package) {
            Box::new(paths.iter().map(|x| x.as_str()))
        } else {
            Box::new(std::iter::empty())
        }
    }

    /// Emit lines constituting this file.
    pub fn as_lines(&self) -> impl Iterator<Item = String> + '_ {
        self.paths.iter().map(|(path, packages)| {
            // BTreeSet doesn't have a .join(). So we need to build a collection that does.
            let packages = packages.iter().map(|s| s.as_str()).collect::<Vec<_>>();

            format!("{}    {}\n", path, packages.join(",'"))
        })
    }

    /// Write the content of this file to a writer.
    ///
    /// Returns the total number of bytes written.
    pub fn write_to(&self, writer: &mut impl Write) -> Result<usize> {
        let mut bytes_count = 0;

        for line in self.as_lines() {
            writer.write_all(line.as_bytes())?;
            bytes_count += line.as_bytes().len();
        }

        Ok(bytes_count)
    }
}

#[derive(Clone, Debug)]
pub struct ContentsFileReader<R> {
    reader: R,
    contents: ContentsFile,
}

impl<R: BufRead> ContentsFileReader<R> {
    /// Create a new instance bound to a reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            contents: ContentsFile::default(),
        }
    }

    /// Consumes the instance, returning the original reader.
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Parse the entirety of the source reader.
    pub fn read_all(&mut self) -> Result<usize> {
        let mut bytes_read = 0;

        while let Ok(read_size) = self.read_line() {
            if read_size == 0 {
                break;
            }

            bytes_read += read_size;
        }

        Ok(bytes_read)
    }

    /// Read and parse a single line from the reader.
    pub fn read_line(&mut self) -> Result<usize> {
        let mut line = String::new();
        let read_size = self.reader.read_line(&mut line)?;

        if read_size != 0 {
            self.contents.parse_and_add_line(&line)?;
        }

        Ok(read_size)
    }

    /// Consume the instance and return the inner [ContentsFile] and the reader.
    pub fn consume(self) -> (ContentsFile, R) {
        (self.contents, self.reader)
    }
}

#[pin_project]
pub struct ContentsFileAsyncReader<R> {
    #[pin]
    reader: R,
    contents: ContentsFile,
}

impl<R> ContentsFileAsyncReader<R>
where
    R: AsyncBufRead + Unpin,
{
    /// Create a new instance bound to a reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            contents: ContentsFile::default(),
        }
    }

    /// Consumes self, returning the inner reader.
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Parse the entirety of the source reader.
    pub async fn read_all(&mut self) -> Result<usize> {
        let mut bytes_read = 0;

        while let Ok(read_size) = self.read_line().await {
            if read_size == 0 {
                break;
            }

            bytes_read += read_size;
        }

        Ok(bytes_read)
    }

    /// Read and parse a single line from the reader.
    pub async fn read_line(&mut self) -> Result<usize> {
        let mut line = String::new();
        let read_size = self.reader.read_line(&mut line).await?;

        if read_size != 0 {
            self.contents.parse_and_add_line(&line)?;
        }

        Ok(read_size)
    }

    /// Consume the instance and return the inner [ContentsFile] and source reader.
    pub fn consume(self) -> (ContentsFile, R) {
        (self.contents, self.reader)
    }
}
