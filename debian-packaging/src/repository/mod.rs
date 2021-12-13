// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian repository primitives.

A Debian repository is a collection of files holding packages and other
support primitives. See <https://wiki.debian.org/DebianRepository/Format>
for the canonical definition of a Debian repository.
*/

use {
    crate::{
        binary_package_control::BinaryPackageControlFile,
        binary_package_list::BinaryPackageList,
        control::ControlParagraphAsyncReader,
        deb::reader::BinaryPackageReader,
        error::{DebianError, Result},
        io::{drain_reader, Compression, ContentDigest, DataResolver},
        repository::{
            contents::{ContentsFile, ContentsFileAsyncReader},
            release::{
                ChecksumType, ContentsFileEntry, PackagesFileEntry, ReleaseFile, SourcesFileEntry,
            },
        },
    },
    async_trait::async_trait,
    futures::{AsyncRead, AsyncReadExt, StreamExt, TryStreamExt},
    std::{borrow::Cow, collections::HashMap, pin::Pin},
};

pub mod builder;
pub mod contents;
pub mod filesystem;
#[cfg(feature = "http")]
pub mod http;
pub mod release;

/// Describes how to fetch a binary package from a repository.
#[derive(Clone, Debug)]
pub struct BinaryPackageFetch<'a> {
    /// The binary package control paragraph from which this entry came.
    pub control_file: BinaryPackageControlFile<'a>,
    /// The relative path of this binary package.
    ///
    /// Corresponds to the `Filename` field.
    pub path: String,
    /// The expected size of the retrieved file.
    pub size: u64,
    /// The expected content digest of the retrieved file.
    pub digest: ContentDigest,
}

/// Debian repository reader bound to the root of the repository.
///
/// This trait facilitates access to *pool* as well as to multiple
/// *releases* within the repository.
#[async_trait]
pub trait RepositoryRootReader: DataResolver + Sync {
    /// Obtain the URL to which this reader is bound.  
    fn url(&self) -> Result<url::Url>;

    /// Obtain a [ReleaseReader] for a given distribution.
    ///
    /// This assumes the `InRelease` file is located in `dists/{distribution}/`. This is the case
    /// for most repositories.
    async fn release_reader(&self, distribution: &str) -> Result<Box<dyn ReleaseReader>> {
        self.release_reader_with_distribution_path(&format!(
            "dists/{}",
            distribution.trim_matches('/')
        ))
        .await
    }

    /// Obtain a [ReleaseReader] given a distribution path.
    ///
    /// Typically distributions exist at `dists/<distribution>/`. However, this may not
    /// always be the case. This method allows explicitly passing in the relative path
    /// holding the `InRelease` file.
    async fn release_reader_with_distribution_path(
        &self,
        path: &str,
    ) -> Result<Box<dyn ReleaseReader>>;

    /// Fetch and parse an `InRelease` file at the relative path specified.
    ///
    /// `path` is typically a value like `dists/<distribution>/InRelease`. e.g.
    /// `dists/bullseye/InRelease`.
    ///
    /// The default implementation of this trait should be sufficient for most types.
    async fn fetch_inrelease(&self, path: &str) -> Result<ReleaseFile<'static>> {
        let mut reader = self.get_path(path).await?;

        let mut data = vec![];
        reader.read_to_end(&mut data).await?;

        Ok(ReleaseFile::from_armored_reader(std::io::Cursor::new(
            data,
        ))?)
    }

    /// Fetch a binary package given a [BinaryPackageFetch] instruction.
    ///
    /// Returns a generic [AsyncRead] to obtain the raw file content.
    async fn fetch_binary_package_generic<'fetch>(
        &self,
        fetch: BinaryPackageFetch<'fetch>,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>> {
        self.get_path_with_digest_verification(&fetch.path, fetch.size, fetch.digest)
            .await
    }

    /// Fetch a binary package given a [BinaryPackageFetch] instruction.
    ///
    /// Returns a [BinaryPackageReader] capable of parsing the package.
    ///
    /// Due to limitations in [BinaryPackageReader], the entire package content is buffered
    /// in memory and isn't read lazily.
    async fn fetch_binary_package_deb_reader<'fetch>(
        &self,
        fetch: BinaryPackageFetch<'fetch>,
    ) -> Result<BinaryPackageReader<std::io::Cursor<Vec<u8>>>> {
        let mut reader = self.fetch_binary_package_generic(fetch).await?;
        // TODO implement an async reader.
        let mut buf = vec![];
        reader.read_to_end(&mut buf).await?;

        Ok(BinaryPackageReader::new(std::io::Cursor::new(buf))?)
    }
}

/// Provides a transport-agnostic mechanism for reading from a parsed `[In]Release` file.
#[async_trait]
pub trait ReleaseReader: DataResolver + Sync {
    /// Obtain the base URL to which this instance is bound.
    fn url(&self) -> Result<url::Url>;

    /// Obtain the parsed `[In]Release` file from which this reader is derived.
    fn release_file(&self) -> &ReleaseFile<'_>;

    /// Obtain the checksum flavor of content to retrieve.
    ///
    /// By default, this will prefer the strongest known checksum advertised in the
    /// release file.
    fn retrieve_checksum(&self) -> Result<ChecksumType> {
        let release = self.release_file();

        let checksum = &[ChecksumType::Sha256, ChecksumType::Sha1, ChecksumType::Md5]
            .iter()
            .find(|variant| release.as_ref().field(variant.field_name()).is_some())
            .ok_or(DebianError::RepositoryReadReleaseNoKnownChecksum)?;

        Ok(**checksum)
    }

    /// Obtain the preferred compression format to retrieve index files in.
    fn preferred_compression(&self) -> Compression;

    /// Set the preferred compression format for retrieved index files.
    ///
    /// Index files are often published in multiple compression formats, including no
    /// compression. This function can be used to instruct the reader which compression
    /// format to prefer.
    fn set_preferred_compression(&mut self, compression: Compression);

    /// Obtain parsed `Packages` file entries within this Release file.
    ///
    /// Only entries for the checksum as defined by [Self::retrieve_checksum()] are returned.
    ///
    /// There may be multiple entries for a given logical `Packages` file corresponding
    /// to different compression formats. Use [Self::packages_entry()] to resolve the entry
    /// for the `Packages` file for the preferred configuration.
    fn packages_indices_entries(&self) -> Result<Vec<PackagesFileEntry<'_>>> {
        Ok(
            if let Some(entries) = self
                .release_file()
                .iter_packages_indices(self.retrieve_checksum()?)
            {
                entries.collect::<Result<Vec<_>>>()?
            } else {
                vec![]
            },
        )
    }

    /// Like [Self::packages_indices_entries()] except it deduplicates entries.
    ///
    /// If there are multiple entries for a `Packages` file with varying compression, the most
    /// preferred compression format is returned.
    fn packages_indices_entries_preferred_compression(&self) -> Result<Vec<PackagesFileEntry<'_>>> {
        let mut entries = HashMap::new();

        for entry in self.packages_indices_entries()? {
            entries
                .entry((
                    entry.component.clone(),
                    entry.architecture.clone(),
                    entry.is_installer,
                ))
                .or_insert_with(Vec::new)
                .push(entry);
        }

        entries
            .into_values()
            .map(|candidates| {
                if let Some(entry) = candidates
                    .iter()
                    .find(|entry| entry.compression == self.preferred_compression())
                {
                    Ok(entry.clone())
                } else {
                    for compression in Compression::default_preferred_order() {
                        if let Some(entry) = candidates
                            .iter()
                            .find(|entry| entry.compression == compression)
                        {
                            return Ok(entry.clone());
                        }
                    }

                    Err(DebianError::RepositoryReadPackagesIndicesEntryNotFound)
                }
            })
            .collect::<Result<Vec<_>>>()
    }

    /// Resolve indices for `Contents` files.
    ///
    /// Only entries for the checksum as defined by [Self::retrieve_checksum()] are returned.
    ///
    /// Multiple entries for the same logical file with varying compression formats may be
    /// returned.
    fn contents_indices_entries(&self) -> Result<Vec<ContentsFileEntry<'_>>> {
        Ok(
            if let Some(entries) = self
                .release_file()
                .iter_contents_indices(self.retrieve_checksum()?)
            {
                entries.collect::<Result<Vec<_>>>()?
            } else {
                vec![]
            },
        )
    }

    /// Resolve indices for `Sources` file.
    ///
    /// Only entries for the checksum as defined by [Self::retrieve_checksum()] are returned.
    ///
    /// Multiple entries for the same logical file with varying compression formats may be
    /// returned.
    fn sources_indices_entries(&self) -> Result<Vec<SourcesFileEntry<'_>>> {
        Ok(
            if let Some(entries) = self
                .release_file()
                .iter_sources_indices(self.retrieve_checksum()?)
            {
                entries.collect::<Result<Vec<_>>>()?
            } else {
                vec![]
            },
        )
    }

    /// Resolve a reference to a `Packages` file to fetch given search criteria.
    ///
    /// This will find all entries defining the desired `Packages` file. It will filter
    /// through the [ChecksumType] as defined by [Self::retrieve_checksum()] and will prioritize
    /// the compression format according to [Self::preferred_compression()].
    fn packages_entry(
        &self,
        component: &str,
        architecture: &str,
        is_installer: bool,
    ) -> Result<PackagesFileEntry<'_>> {
        self.packages_indices_entries_preferred_compression()?
            .into_iter()
            .find(|entry| {
                entry.component == component
                    && entry.architecture == architecture
                    && entry.is_installer == is_installer
            })
            .ok_or(DebianError::RepositoryReadPackagesIndicesEntryNotFound)
    }

    /// Fetch and parse a `Packages` file described by a [PackagesFileEntry].
    async fn resolve_packages_from_entry<'entry, 'slf: 'entry>(
        &'slf self,
        entry: &'entry PackagesFileEntry<'slf>,
    ) -> Result<BinaryPackageList<'static>> {
        let release = self.release_file();

        let path = if release.acquire_by_hash().unwrap_or_default() {
            entry.entry.by_hash_path()
        } else {
            entry.entry.path.to_string()
        };

        let mut reader = ControlParagraphAsyncReader::new(futures::io::BufReader::new(
            self.get_path_decoded_with_digest_verification(
                &path,
                entry.compression,
                entry.entry.size,
                entry.entry.digest.as_content_digest()?,
            )
            .await?,
        ));

        let mut res = BinaryPackageList::default();

        while let Some(paragraph) = reader.read_paragraph().await? {
            res.push(BinaryPackageControlFile::from(paragraph));
        }

        Ok(res)
    }

    /// Resolve packages given parameters to resolve a `Packages` file.
    async fn resolve_packages(
        &self,
        component: &str,
        arch: &str,
        is_installer: bool,
    ) -> Result<BinaryPackageList<'static>> {
        let entry = self.packages_entry(component, arch, is_installer)?;

        self.resolve_packages_from_entry(&entry).await
    }

    /// Retrieve fetch instructions for binary packages.
    ///
    /// The caller can specify a filter function to choose which packages to retrieve.
    /// Filtering works in 2 stages.
    ///
    /// First, `packages_file_filter` is called with each [ReleaseFileEntry] defining
    /// a `Packages*` file. If the filter returns true, this list of packages will be
    /// retrieved and expanded.
    ///
    /// Second, `binary_package_filter` is called for each binary package entry seen
    /// in parsed `Packages*` files. If the function returns true, this binary package
    /// will be retrieved.
    ///
    /// The emitted values can be fed into [RepositoryRootReader::fetch_binary_package_generic()]
    /// and [RepositoryRootReader::fetch_binary_package_deb_reader()] to fetch the binary package
    /// content.
    async fn resolve_package_fetches(
        &self,
        packages_file_filter: Box<dyn (Fn(PackagesFileEntry) -> bool) + Send>,
        binary_package_filter: Box<dyn (Fn(BinaryPackageControlFile) -> bool) + Send>,
        threads: usize,
    ) -> Result<Vec<BinaryPackageFetch<'_>>> {
        let packages_entries = self.packages_indices_entries_preferred_compression()?;

        let fs = packages_entries
            .iter()
            .filter(|entry| packages_file_filter((*entry).clone()))
            .map(|entry| self.resolve_packages_from_entry(entry))
            .collect::<Vec<_>>();

        let mut packages_fs = futures::stream::iter(fs).buffer_unordered(threads);

        let mut fetches = vec![];

        while let Some(pl) = packages_fs.try_next().await? {
            for cf in pl.into_iter() {
                // Needed by IDE for type hinting for some reason.
                let cf: BinaryPackageControlFile = cf;

                if binary_package_filter(cf.clone()) {
                    let path = cf.as_ref().required_field_str("Filename")?.to_string();

                    let size = cf.as_ref().field_u64("Size").ok_or_else(|| {
                        DebianError::ControlRequiredFieldMissing("Size".to_string())
                    })??;

                    let digest = ChecksumType::preferred_order()
                        .find_map(|checksum| {
                            cf.as_ref()
                                .field_str(checksum.field_name())
                                .map(|hex_digest| {
                                    ContentDigest::from_hex_checksum(checksum, hex_digest)
                                })
                        })
                        .ok_or(DebianError::RepositoryReadCouldNotDeterminePackageDigest)??;

                    fetches.push(BinaryPackageFetch {
                        control_file: cf,
                        path,
                        size,
                        digest,
                    });
                }
            }
        }

        Ok(fetches)
    }

    /// Resolve a reference to a `Contents` file to fetch given search criteria.
    ///
    /// This will attempt to find the entry for a `Contents` file given search criteria.
    fn contents_entry(
        &self,
        component: &str,
        architecture: &str,
        is_installer: bool,
    ) -> Result<ContentsFileEntry> {
        let entries = self
            .contents_indices_entries()?
            .into_iter()
            .filter(|entry| {
                entry.component == component
                    && entry.architecture == architecture
                    && entry.is_installer == is_installer
            })
            .collect::<Vec<_>>();

        if let Some(entry) = entries
            .iter()
            .find(|entry| entry.compression == self.preferred_compression())
        {
            Ok(entry.clone())
        } else {
            for compression in Compression::default_preferred_order() {
                if let Some(entry) = entries
                    .iter()
                    .find(|entry| entry.compression == compression)
                {
                    return Ok(entry.clone());
                }
            }

            Err(DebianError::RepositoryReadContentsIndicesEntryNotFound)
        }
    }

    async fn resolve_contents(
        &self,
        component: &str,
        architecture: &str,
        is_installer: bool,
    ) -> Result<ContentsFile> {
        let release = self.release_file();
        let entry = self.contents_entry(component, architecture, is_installer)?;

        let path = if release.acquire_by_hash().unwrap_or_default() {
            entry.entry.by_hash_path()
        } else {
            entry.entry.path.to_string()
        };

        let reader = self
            .get_path_decoded_with_digest_verification(
                &path,
                entry.compression,
                entry.entry.size,
                entry.entry.digest.as_content_digest()?,
            )
            .await?;

        let mut reader = ContentsFileAsyncReader::new(futures::io::BufReader::new(reader));
        reader.read_all().await?;

        let (contents, reader) = reader.consume();

        drain_reader(reader)
            .await
            .map_err(|e| DebianError::RepositoryIoPath(path, e))?;

        Ok(contents)
    }
}

/// Describes a repository path verification state.
#[derive(Clone, Copy, Debug)]
pub enum RepositoryPathVerificationState {
    /// The path exists but its integrity was not verified.
    ExistsNoIntegrityCheck,
    /// The path exists and its integrity was verified.
    ExistsIntegrityVerified,
    /// The path exists and its integrity didn't match expectations.
    ExistsIntegrityMismatch,
    /// The path is missing.
    Missing,
}

/// Represents the result of a repository path verification check.
#[derive(Clone, Debug)]
pub struct RepositoryPathVerification<'a> {
    /// The path that was tested.
    pub path: &'a str,
    /// The state of the path.
    pub state: RepositoryPathVerificationState,
}

impl<'a> std::fmt::Display for RepositoryPathVerification<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.state {
            RepositoryPathVerificationState::ExistsNoIntegrityCheck => {
                write!(f, "{} exists (no integrity check performed)", self.path)
            }
            RepositoryPathVerificationState::ExistsIntegrityVerified => {
                write!(f, "{} exists (integrity verified)", self.path)
            }
            RepositoryPathVerificationState::ExistsIntegrityMismatch => {
                write!(f, "{} exists (integrity mismatch!)", self.path)
            }
            RepositoryPathVerificationState::Missing => {
                write!(f, "{} missing", self.path)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct RepositoryWrite<'a> {
    /// The path that was written.
    pub path: Cow<'a, str>,
    /// The number of bytes written.
    pub bytes_written: u64,
}

#[async_trait]
pub trait RepositoryWriter: Sync {
    /// Verify the existence of a path with optional content integrity checking.
    ///
    /// If the size and digest are [Some] implementations *may* perform additional
    /// content integrity verification. Or they may not. They should not lie about
    /// whether integrity verification was performed in the returned value, however.
    async fn verify_path<'path>(
        &self,
        path: &'path str,
        expected_content: Option<(u64, ContentDigest)>,
    ) -> Result<RepositoryPathVerification<'path>>;

    /// Write data to a given path.
    ///
    /// The data to write is provided by an [AsyncRead] reader.
    async fn write_path<'path, 'reader>(
        &self,
        path: Cow<'path, str>,
        reader: Pin<Box<dyn AsyncRead + Send + 'reader>>,
    ) -> Result<RepositoryWrite<'path>>;
}
