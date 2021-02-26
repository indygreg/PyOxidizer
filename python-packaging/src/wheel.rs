// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Interact with Python wheel files. */

use {
    crate::{
        filesystem_scanning::PythonResourceIterator, module_util::PythonModuleSuffixes,
        package_metadata::PythonPackageMetadata, resource::PythonResource,
    },
    anyhow::{anyhow, Context, Result},
    once_cell::sync::Lazy,
    std::{
        borrow::Cow,
        io::Read,
        path::{Path, PathBuf},
    },
    tugger_file_manifest::{File, FileEntry, FileManifest},
    zip::ZipArchive,
};

/// Regex for finding the wheel info directory.
///
/// This is copied from the wheel.wheelfile Python module.

static RE_WHEEL_INFO: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"^(?P<namever>(?P<name>.+?)-(?P<ver>.+?))(-(?P<build>\d[^-]*))?-(?P<pyver>.+?)-(?P<abi>.+?)-(?P<plat>.+?)\.whl$").unwrap()
});

const S_IXUSR: u32 = 64;

/// Represents a Python wheel archive.
pub struct WheelArchive {
    files: FileManifest,
    name_version: String,
}

impl WheelArchive {
    /// Construct an instance from a generic reader.
    ///
    /// `basename` is the filename of the wheel. It is used to try to
    /// locate the info directory.
    pub fn from_reader<R>(reader: R, basename: &str) -> Result<Self>
    where
        R: std::io::Read + std::io::Seek,
    {
        let captures = RE_WHEEL_INFO
            .captures(basename)
            .ok_or_else(|| anyhow!("failed to parse wheel basename: {}", basename))?;

        let name_version = captures
            .name("namever")
            .ok_or_else(|| anyhow!("could not find name-version in wheel name"))?
            .as_str()
            .to_string();

        let mut archive = ZipArchive::new(reader)?;

        let mut files = FileManifest::default();

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let mut buffer = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut buffer)?;

            files.add_file_entry(
                Path::new(file.name()),
                FileEntry {
                    data: buffer.into(),
                    executable: file.unix_mode().unwrap_or(0) & S_IXUSR != 0,
                },
            )?;
        }

        Ok(Self {
            files,
            name_version,
        })
    }

    /// Construct an instance from a filesystem path.
    pub fn from_path(path: &Path) -> Result<Self> {
        let fh = std::fs::File::open(path)
            .with_context(|| format!("opening {} for wheel reading", path.display()))?;

        let reader = std::io::BufReader::new(fh);
        let basename = path
            .file_name()
            .ok_or_else(|| anyhow!("could not derive file name"))?
            .to_string_lossy();

        Self::from_reader(reader, &basename)
    }

    fn dist_info_path(&self) -> String {
        format!("{}.dist-info", self.name_version)
    }

    fn data_path(&self) -> String {
        format!("{}.data", self.name_version)
    }

    /// Obtain metadata about the wheel archive itself.
    pub fn archive_metadata(&self) -> Result<PythonPackageMetadata> {
        let path = format!("{}/WHEEL", self.dist_info_path());

        let file = self
            .files
            .get(&path)
            .ok_or_else(|| anyhow!("{} does not exist", path))?;

        PythonPackageMetadata::from_metadata(&file.data.resolve()?)
    }

    /// Obtain the `.dist-info/METADATA` content as a parsed object.
    pub fn metadata(&self) -> Result<PythonPackageMetadata> {
        let path = format!("{}/METADATA", self.dist_info_path());

        let file = self
            .files
            .get(&path)
            .ok_or_else(|| anyhow!("{} does not exist", path))?;

        PythonPackageMetadata::from_metadata(&file.data.resolve()?)
    }

    /// Obtain the first header value from the archive metadata file.
    pub fn archive_metadata_header(&self, header: &str) -> Result<Cow<str>> {
        let metadata = self.archive_metadata()?;

        Ok(Cow::Owned(
            metadata
                .find_first_header(header)
                .ok_or_else(|| anyhow!("{} not found", header))?
                .to_string(),
        ))
    }

    /// Obtain values of all headers from the archive metadata file.
    pub fn archive_metadata_headers(&self, header: &str) -> Result<Vec<Cow<str>>> {
        let metadata = self.archive_metadata()?;

        Ok(metadata
            .find_all_headers(header)
            .iter()
            .map(|s| Cow::Owned(s.to_string()))
            .collect::<Vec<_>>())
    }

    /// Obtain the version number of the wheel specification used to build this wheel.
    pub fn wheel_version(&self) -> Result<Cow<str>> {
        self.archive_metadata_header("Wheel-Version")
    }

    /// Obtain the generator of the wheel archive.
    pub fn wheel_generator(&self) -> Result<Cow<str>> {
        self.archive_metadata_header("Generator")
    }

    /// Whether `Root-Is-Purelib` is set.
    pub fn root_is_purelib(&self) -> Result<bool> {
        Ok(self.archive_metadata_header("Root-Is-Purelib")? == "true")
    }

    /// `Tag` values for the wheel archive.
    pub fn tags(&self) -> Result<Vec<Cow<str>>> {
        self.archive_metadata_headers("Tag")
    }

    /// `Build` identifier for the wheel archive.
    pub fn build(&self) -> Result<Cow<str>> {
        self.archive_metadata_header("Build")
    }

    /// `Install-Paths-To` values.
    pub fn install_paths_to(&self) -> Result<Vec<Cow<str>>> {
        self.archive_metadata_headers("Install-Paths-To")
    }

    /// Obtain files in the .dist-info/ directory.
    ///
    /// The returned `PathBuf` are prefixed with the appropriate `*.dist-info`
    /// directory.
    pub fn dist_info_files(&self) -> Vec<File> {
        let prefix = format!("{}/", self.dist_info_path());
        self.files
            .iter_files()
            .filter(|f| f.path.starts_with(&prefix))
            .collect::<Vec<_>>()
    }

    /// Obtain paths in a `.data/*/` directory.
    fn data_paths(&self, key: &str) -> Vec<File> {
        let prefix = format!("{}.data/{}/", self.name_version, key);

        self.files
            .iter_files()
            .filter_map(|f| {
                if f.path.starts_with(&prefix) {
                    Some(File {
                        path: PathBuf::from(&f.path.display().to_string()[prefix.len()..]),
                        entry: f.entry,
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }

    /// Obtain files that should be installed to `purelib`.
    ///
    /// `*.data/purelib/` prefix is stripped from returned `PathBuf`.
    pub fn purelib_files(&self) -> Vec<File> {
        self.data_paths("purelib")
    }

    /// Obtain files that should be installed to `platlib`.
    ///
    /// `*.data/platlib/` prefix is stripped from returned `PathBuf`.
    pub fn platlib_files(&self) -> Vec<File> {
        self.data_paths("platlib")
    }

    /// Obtain files that should be installed to `headers`.
    ///
    /// `*.data/headers/` prefix is stripped from returned `PathBuf`.
    pub fn headers_files(&self) -> Vec<File> {
        self.data_paths("headers")
    }

    /// Obtain files that should be installed to `scripts`.
    ///
    /// `*.data/scripts/` prefix is stripped from returned `PathBuf`.
    ///
    /// TODO support optional argument to rewrite `#!python` shebangs.
    pub fn scripts_files(&self) -> Vec<File> {
        self.data_paths("scripts")
    }

    /// Obtain files that should be installed to `data`.
    ///
    /// `*.data/data/` prefix is stripped from returned `PathBuf`.
    pub fn data_files(&self) -> Vec<File> {
        self.data_paths("data")
    }

    /// Obtain normal files not part of metadata or special files.
    ///
    /// These are likely installed as-is.
    ///
    /// The returned `PathBuf` has the same path as the file in the
    /// wheel archive.
    pub fn regular_files(&self) -> Vec<File> {
        let dist_info_prefix = format!("{}/", self.dist_info_path());
        let data_prefix = format!("{}/", self.data_path());

        self.files
            .iter_files()
            .filter(|f| {
                !(f.path.starts_with(&dist_info_prefix) || f.path.starts_with(&data_prefix))
            })
            .collect::<Vec<_>>()
    }

    /// Obtain `PythonResource` for files within the wheel.
    pub fn python_resources<'a>(
        &self,
        cache_tag: &str,
        suffixes: &PythonModuleSuffixes,
        emit_files: bool,
        classify_files: bool,
    ) -> Result<Vec<PythonResource<'a>>> {
        // The filesystem scanning code relies on the final install layout.
        // So we need to simulate that.

        // Regular files are as-is.
        let mut inputs = self.regular_files();

        // As are .dist-info paths.
        inputs.extend(self.dist_info_files());

        // Get modules from purelib and platlib, remapping them to the root.
        inputs.extend(self.purelib_files());
        inputs.extend(self.platlib_files());

        // Get resources from data, remapping them to the root.
        inputs.extend(self.data_files());

        // Other data keys are `headers` and `scripts`, which we don't yet
        // support as resource types.

        PythonResourceIterator::from_data_locations(
            &inputs,
            cache_tag,
            suffixes,
            emit_files,
            classify_files,
        )?
        .collect::<Result<Vec<_>>>()
    }
}
