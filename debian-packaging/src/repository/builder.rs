// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Build your own Debian repositories.

This module defines functionality for constructing Debian repositories.

See <https://wiki.debian.org/DebianRepository/Format> for the format of repositories.

Repositories are essentially a virtual filesystem composed of some well-defined files.
Primitives in this module facilitate constructing your own repositories.
*/

use {
    crate::{
        binary_package_control::BinaryPackageControlFile,
        control::{ControlField, ControlFieldValue, ControlParagraph},
        deb::reader::resolve_control_file,
        error::{DebianError, Result},
        io::{read_compressed, ContentDigest, DataResolver, MultiContentDigest, MultiDigester},
        pgp::cleartext_sign,
        repository::{
            release::{ChecksumType, ReleaseFile, DATE_FORMAT},
            Compression, RepositoryPathVerificationState, RepositoryWriter,
        },
    },
    chrono::{DateTime, Utc},
    futures::{AsyncRead, AsyncReadExt, StreamExt, TryStreamExt},
    pgp::{crypto::HashAlgorithm, types::SecretKeyTrait},
    std::{
        borrow::Cow,
        collections::{BTreeMap, BTreeSet, HashMap},
        pin::Pin,
        str::FromStr,
    },
};

/// Describes the layout of the `pool` part of the repository.
///
/// This type effectively controls where `.deb` files will be placed under the repository root.
#[derive(Clone, Copy, Debug)]
pub enum PoolLayout {
    /// File paths are `<component>/<name_prefix>/<filename>`.
    ///
    /// This is the layout as used by the Debian distribution.
    ///
    /// The package name is used to derive a directory prefix. For packages beginning with `lib`,
    /// the prefix is `libz/<package>/`. For everything else, it is `<first character>/<package>/`.
    ///
    /// For example, file `zstd_1.4.8+dfsg-2.1_amd64.deb` in the `main` component will be mapped to
    /// `pool/main/libz/libzstd/zstd_1.4.8+dfsg-2.1_amd64.deb` and `python3.9_3.9.9-1_arm64.deb` in
    /// the `main` component will be mapped to `pool/main/p/python3.9/python3.9_3.9.9-1_arm64.deb`.
    ComponentThenNamePrefix,
}

impl Default for PoolLayout {
    fn default() -> Self {
        Self::ComponentThenNamePrefix
    }
}

impl PoolLayout {
    /// Compute the path to a file given the source package name and its filename.
    pub fn path(&self, component: &str, package: &str, filename: &str) -> String {
        match self {
            Self::ComponentThenNamePrefix => {
                let name_prefix = if package.starts_with("lib") {
                    format!("{}/{}", &package[0..4], package)
                } else {
                    format!("{}/{}", &package[0..1], package)
                };

                format!("pool/{}/{}/{}", component, name_prefix, filename)
            }
        }
    }
}

/// Describes a reference to a `.deb` Debian package existing somewhere.
///
/// This trait is used as a generic way to refer to a `.deb` package, without implementations
/// necessarily having immediate access to the full content/data of that `.deb` package.
pub trait DebPackageReference<'cf> {
    /// Obtain the size in bytes of the `.deb` file.
    ///
    /// This becomes the `Size` field in `Packages*` control files.
    fn deb_size_bytes(&self) -> Result<usize>;

    /// Obtains the binary digest of this file given a checksum flavor.
    ///
    /// Implementations can compute the digest at run-time or return a cached value.
    fn deb_digest(&self, checksum: ChecksumType) -> Result<ContentDigest>;

    /// Obtain the filename of this `.deb`.
    ///
    /// This should be just the file name, without any directory components.
    fn deb_filename(&self) -> Result<String>;

    /// Obtain a [BinaryPackageControlFile] representing content for a `Packages` index file.
    ///
    /// The returned content can come from a `control` file in a `control.tar` or from
    /// an existing `Packages` control file.
    ///
    /// The control file must have at least `Package`, `Version`, and `Architecture` fields.
    fn control_file_for_packages_index(&self) -> Result<BinaryPackageControlFile<'cf>>;
}

/// Holds the content of a `.deb` file in-memory.
pub struct InMemoryDebFile {
    filename: String,
    data: Vec<u8>,
}

impl InMemoryDebFile {
    /// Create a new instance bound to memory.
    pub fn new(filename: String, data: Vec<u8>) -> Self {
        Self { filename, data }
    }
}

impl<'cf> DebPackageReference<'cf> for InMemoryDebFile {
    fn deb_size_bytes(&self) -> Result<usize> {
        Ok(self.data.len())
    }

    fn deb_digest(&self, checksum: ChecksumType) -> Result<ContentDigest> {
        let mut h = checksum.new_hasher();
        h.update(&self.data);
        let digest = h.finish().to_vec();

        Ok(match checksum {
            ChecksumType::Md5 => ContentDigest::Md5(digest),
            ChecksumType::Sha1 => ContentDigest::Sha1(digest),
            ChecksumType::Sha256 => ContentDigest::Sha256(digest),
        })
    }

    fn deb_filename(&self) -> Result<String> {
        Ok(self.filename.clone())
    }

    fn control_file_for_packages_index(&self) -> Result<BinaryPackageControlFile<'cf>> {
        Ok(resolve_control_file(std::io::Cursor::new(&self.data))?)
    }
}

/// Describes an index file to write.
pub struct IndexFileReader<'a> {
    /// Provides the uncompressed content of the file.
    pub reader: Pin<Box<dyn AsyncRead + Send + 'a>>,
    /// The compression to apply to the written file.
    pub compression: Compression,
    /// The directory the index file is based in.
    pub directory: String,
    /// The filename of the index file (without the compression suffix).
    pub filename: String,
}

impl<'a> IndexFileReader<'a> {
    /// Obtain the canonical path of this entry as it would appear in an `[In]Release` file.
    pub fn canonical_path(&self) -> String {
        format!(
            "{}/{}{}",
            self.directory,
            self.filename,
            self.compression.extension()
        )
    }

    /// Obtain the `by-hash` path given a [ContentDigest].
    pub fn by_hash_path(&self, digest: &ContentDigest) -> String {
        format!(
            "{}/by-hash/{}/{}",
            self.directory,
            digest.release_field_name(),
            digest.digest_hex()
        )
    }
}

struct ExpandedIndexFile {
    canonical_path: String,
    write_path: String,
    digests: MultiContentDigest,
    data: Vec<u8>,
}

/// Describes a file in the *pool* to support a binary package.
#[derive(Debug)]
pub struct BinaryPackagePoolArtifact<'a> {
    /// The file path relative to the repository root.
    pub path: &'a str,
    /// The expected size of the file.
    pub size: usize,
    /// The expected digest of the file.
    pub digest: ContentDigest,
}

/// Represents a publishing event.
pub enum PublishEvent {
    ResolvedPoolArtifacts(usize),

    /// A pool artifact with the given path is current and was not updated.
    PoolArtifactCurrent(String),

    /// A pool artifact with the given path is missing and will be created.
    PoolArtifactMissing(String),

    /// Total number of pool artifacts to publish.
    PoolArtifactsToPublish(usize),

    /// A pool artifact with the given path and size was created.
    PoolArtifactCreated(String, usize),

    /// The path to an index file to write.
    IndexFileToWrite(String),

    /// An index file that was written.
    IndexFileWritten(String, u64),
}

impl std::fmt::Display for PublishEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResolvedPoolArtifacts(count) => {
                write!(f, "resolved {} needed pool artifacts", count)
            }
            Self::PoolArtifactCurrent(path) => {
                write!(f, "pool path {} is present", path)
            }
            Self::PoolArtifactMissing(path) => {
                write!(f, "pool path {} will be written", path)
            }
            Self::PoolArtifactsToPublish(count) => {
                write!(f, "{} pool artifacts will be written", count)
            }
            Self::PoolArtifactCreated(path, size) => {
                write!(f, "wrote {} bytes to {}", size, path)
            }
            Self::IndexFileToWrite(path) => {
                write!(f, "index file {} will be written", path)
            }
            Self::IndexFileWritten(path, size) => {
                write!(f, "wrote {} bytes to {}", size, path)
            }
        }
    }
}

// (Package, Version) -> paragraph.
type IndexedBinaryPackages<'a> = BTreeMap<(String, String), ControlParagraph<'a>>;

// (component, architecture) -> packages.
type ComponentBinaryPackages<'a> = BTreeMap<(String, String), IndexedBinaryPackages<'a>>;

/// Build Debian repositories from scratch.
///
/// Instances of this type are used to iteratively construct a Debian repository.
///
/// A Debian repository consists of named *components* holding binary packages, sources,
/// installer packages, and metadata gluing it all together.
///
/// # Usage
///
/// Instances are constructed, preferably via [Self::new_recommended()].
///
/// Additional metadata about the repository is then registered using the following functions
/// (as needed):
///
/// * [Self::add_architecture()]
/// * [Self::add_component()]
/// * [Self::add_checksum()]
/// * [Self::set_suite()]
/// * [Self::set_codename()]
/// * [Self::set_date()]
/// * [Self::set_valid_until()]
/// * [Self::set_description()]
/// * [Self::set_origin()]
/// * [Self::set_label()]
/// * [Self::set_version()]
/// * [Self::set_acquire_by_hash()]
///
/// See <https://wiki.debian.org/DebianRepository/Format> for a description of what these various
/// fields are used for.
///
/// After basic metadata is in place, `.deb` packages are registered against the builder via
/// [Self::add_binary_deb()].
///
/// Once everything is registered against the builder, it is time to *publish* (read: write)
/// the repository content.
///
/// Publishing works by first writing *pool* content. The *pool* is an area of the repository
/// where blobs (like `.deb` packages) are stored. To publish the pool, call
/// [Self::publish_pool_artifacts()]. This takes a [DataResolver] for obtaining missing pool
/// content. Its content retrieval functions will be called for each pool path that needs to be
/// copied to the writer. Since [crate::repository::RepositoryRootReader] must implement
/// [DataResolver], you can pass an instance as the [DataResolver] to effectively copy
/// artifacts from another Debian repository. Note: for this to work, the source repository
/// must have the same [PoolLayout] as this repository. This may not always be the case!
/// To more robustly copy files from another repository, instantiate a
/// [crate::io::PathMappingDataResolver] and call
/// [crate::io::PathMappingDataResolver::add_path_map()] with the result from
/// [Self::add_binary_deb()] (and similar function) to install a path mapping.
#[derive(Debug, Default)]
pub struct RepositoryBuilder<'cf> {
    // Release file fields.
    architectures: BTreeSet<String>,
    components: BTreeSet<String>,
    suite: Option<String>,
    codename: Option<String>,
    date: Option<DateTime<Utc>>,
    valid_until: Option<DateTime<Utc>>,
    description: Option<String>,
    origin: Option<String>,
    label: Option<String>,
    version: Option<String>,
    acquire_by_hash: Option<bool>,
    checksums: BTreeSet<ChecksumType>,
    pool_layout: PoolLayout,
    index_file_compressions: BTreeSet<Compression>,
    binary_packages: ComponentBinaryPackages<'cf>,
    installer_packages: ComponentBinaryPackages<'cf>,
    source_packages: BTreeMap<String, IndexedBinaryPackages<'cf>>,
    translations: BTreeMap<String, ()>,
}

impl<'cf> RepositoryBuilder<'cf> {
    /// Create a new instance with recommended settings.
    ///
    /// Files that should almost always be set (like `Architectures` and `Components`)
    /// are empty. It is recommended to use [Self::new_recommended()] instead.
    pub fn new_recommended_empty() -> Self {
        Self {
            architectures: BTreeSet::new(),
            components: BTreeSet::new(),
            suite: None,
            codename: None,
            date: Some(Utc::now()),
            valid_until: None,
            description: None,
            origin: None,
            label: None,
            version: None,
            acquire_by_hash: Some(true),
            checksums: BTreeSet::from_iter([ChecksumType::Md5, ChecksumType::Sha256]),
            pool_layout: PoolLayout::default(),
            index_file_compressions: BTreeSet::from_iter([
                Compression::None,
                Compression::Gzip,
                Compression::Xz,
            ]),
            binary_packages: ComponentBinaryPackages::default(),
            installer_packages: ComponentBinaryPackages::default(),
            source_packages: BTreeMap::default(),
            translations: BTreeMap::default(),
        }
    }

    /// Create a new instance with recommended settings and fields.
    ///
    /// The arguments to this function are those that should be defined on most Debian repositories.
    ///
    /// Calling this function is equivalent to calling [Self::new_recommended_empty()] then calling
    /// various `.add_*()` methods on the returned instance.
    pub fn new_recommended(
        architectures: impl Iterator<Item = impl ToString>,
        components: impl Iterator<Item = impl ToString>,
        suite: impl ToString,
        codename: impl ToString,
    ) -> Self {
        Self {
            architectures: BTreeSet::from_iter(architectures.map(|x| x.to_string())),
            components: BTreeSet::from_iter(components.map(|x| x.to_string())),
            suite: Some(suite.to_string()),
            codename: Some(codename.to_string()),
            ..Self::new_recommended_empty()
        }
    }

    /// Register an architecture with the builder.
    ///
    /// This defines which platform architectures there will be packages for.
    ///
    /// Example architecture values are `all`, `amd64`, `arm64`, and `i386`.
    pub fn add_architecture(&mut self, arch: impl ToString) {
        self.architectures.insert(arch.to_string());
    }

    /// Register a named component with the builder.
    ///
    /// Components describe a named subset of the repository. Example names include
    /// `main`, `contrib`, `restricted`, `stable`.
    pub fn add_component(&mut self, name: impl ToString) {
        self.components.insert(name.to_string());
    }

    /// Register a checksum type to emit.
    ///
    /// [ChecksumType::Sha256] should always be used. Adding [ChecksumType::Md5] is
    /// recommended for compatibility with old clients.
    pub fn add_checksum(&mut self, value: ChecksumType) {
        self.checksums.insert(value);
    }

    /// Set the `Suite` value.
    ///
    /// This is often a value like `stable`, `bionic`, `groovy`. Some identifier that helps
    /// identify this repository.
    pub fn set_suite(&mut self, value: impl ToString) {
        self.suite = Some(value.to_string());
    }

    /// Set the `Codename` value.
    ///
    /// This is often a human friendly name to help identify the repository. Example values
    /// include `groovy`, `bullseye`, `bionic`.
    pub fn set_codename(&mut self, value: impl ToString) {
        self.codename = Some(value.to_string());
    }

    /// Set the time this repository was created/updated.
    ///
    /// If not set, the current time will be used automatically.
    pub fn set_date(&mut self, value: DateTime<Utc>) {
        self.date = Some(value);
    }

    /// Set the value for the `Valid-Until` field.
    ///
    /// Clients should not trust this repository after this date.
    pub fn set_valid_until(&mut self, value: DateTime<Utc>) {
        self.valid_until = Some(value);
    }

    /// Set a human friendly description text for this repository.
    pub fn set_description(&mut self, value: impl ToString) {
        self.description = Some(value.to_string());
    }

    /// Set a field indicating the origin of the repository.
    pub fn set_origin(&mut self, value: impl ToString) {
        self.origin = Some(value.to_string());
    }

    /// Set freeform text describing the repository.
    pub fn set_label(&mut self, value: impl ToString) {
        self.label = Some(value.to_string());
    }

    /// Set the version of the release.
    ///
    /// Typically `.` delimited integers.
    pub fn set_version(&mut self, value: impl ToString) {
        self.version = Some(value.to_string());
    }

    /// Set the value of `Acquire-By-Hash`.
    ///
    /// This should be enabled for new repositories.
    pub fn set_acquire_by_hash(&mut self, value: bool) {
        self.acquire_by_hash = Some(value);
    }

    /// Set the [PoolLayout] to use.
    ///
    /// The layout can only be updated before content is added. Once a package has been
    /// indexed, this function will error.
    pub fn set_pool_layout(&mut self, layout: PoolLayout) -> Result<()> {
        if self.have_entries() {
            Err(DebianError::RepositoryBuildPoolLayoutImmutable)
        } else {
            self.pool_layout = layout;
            Ok(())
        }
    }

    fn have_entries(&self) -> bool {
        !self.binary_packages.is_empty()
            || !self.source_packages.is_empty()
            || !self.installer_packages.is_empty()
            || !self.translations.is_empty()
    }

    /// Add a binary package `.deb` to this repository in the given component.
    ///
    /// The package to add is specified as a trait to enable callers to represent Debian
    /// packages differently. For example, the trait members may be implemented by just-in-time
    /// parsing of an actual `.deb` file or by retrieving the data from a cache.
    ///
    /// The specified `component` name must be registered with this instance or an error will
    /// occur.
    ///
    /// Returns the pool path / `Filename` field that this binary package `.deb` will occupy
    /// in the repository.
    pub fn add_binary_deb(
        &mut self,
        component: &str,
        deb: &impl DebPackageReference<'cf>,
    ) -> Result<String> {
        if !self.components.contains(component) {
            return Err(DebianError::RepositoryBuildUnknownComponent(
                component.to_string(),
            ));
        }

        let original_control_file = deb.control_file_for_packages_index()?;

        let package = original_control_file.package()?;
        let version = original_control_file.version_str()?;
        let arch = original_control_file.architecture()?;

        if !self.architectures.contains(arch) {
            return Err(DebianError::RepositoryBuildUnknownArchitecture(
                arch.to_string(),
            ));
        }

        // We iteratively build up the control paragraph for the `Packages` file from the original
        // control file.
        let mut para = ControlParagraph::default();

        // Different packages have different fields and it is effectively impossible to maintain
        // an numeration of all known fields. So, copy over all fields and ignore the special ones,
        // which we handle later.
        for field in original_control_file.as_ref().iter_fields() {
            if ![
                "Description",
                "Filename",
                "Size",
                "MD5sum",
                "SHA1",
                "SHA256",
            ]
            .contains(&field.name())
            {
                para.add_field(field.clone());
            }
        }

        // The `Description` field is a bit wonky in Packages files. Instead of capturing multiline
        // values, `Description` is just the first line and a `Description-md5` contains the md5
        // of the multiline value.
        if let Some(description) = original_control_file.first_field("Description") {
            let description = description.value_str();

            if let Some(index) = description.find('\n') {
                let mut h = ChecksumType::Md5.new_hasher();
                h.update(description.as_bytes());
                h.update(b"\n");
                let digest = h.finish();

                para.add_field_from_string(
                    "Description".into(),
                    (&description[0..index]).to_string().into(),
                )?;
                para.add_field_from_string("Description-md5".into(), hex::encode(digest).into())?;
            } else {
                para.add_field_from_string("Description".into(), description.to_string().into())?;
            }
        }

        // The `Filename` is derived from the pool layout scheme in effect.
        let filename = self.pool_layout.path(
            component,
            if let Some(name) = original_control_file.source() {
                name
            } else {
                package
            },
            &deb.deb_filename()?,
        );
        para.add_field_from_string("Filename".into(), filename.clone().into())?;

        // `Size` shouldn't be in the original control file, since it is a property of the
        // `.deb` in which the control file is embedded.
        para.add_field_from_string("Size".into(), format!("{}", deb.deb_size_bytes()?).into())?;

        // Add all configured digests for this repository.
        for checksum in &self.checksums {
            let digest = deb.deb_digest(*checksum)?;

            para.add_field_from_string(checksum.field_name().into(), digest.digest_hex().into())?;
        }

        let component_key = (component.to_string(), arch.to_string());
        let package_key = (package.to_string(), version.to_string());
        self.binary_packages
            .entry(component_key)
            .or_default()
            .insert(package_key, para);

        Ok(filename)
    }

    /// Obtain all components having binary packages.
    ///
    /// The iterator contains 2-tuples of `(component, architecture)`.
    pub fn binary_package_components(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.binary_packages
            .keys()
            .map(|(a, b)| (a.as_str(), b.as_str()))
    }

    /// Obtain an iterator of [ControlParagraph] for binary packages in a given component + architecture.
    ///
    /// This method forms the basic building block for constructing `Packages` files. `Packages`
    /// files can be built by serializing the [ControlParagraph] to a string/writer.
    pub fn iter_component_binary_packages(
        &self,
        component: impl ToString,
        architecture: impl ToString,
    ) -> Box<dyn Iterator<Item = &'_ ControlParagraph> + Send + '_> {
        if let Some(packages) = self
            .binary_packages
            .get(&(component.to_string(), architecture.to_string()))
        {
            Box::new(packages.values())
        } else {
            Box::new(std::iter::empty())
        }
    }

    /// Obtain an iterator of pool artifacts for binary packages that will need to exist.
    pub fn iter_component_binary_package_pool_artifacts(
        &self,
        component: impl ToString,
        architecture: impl ToString,
    ) -> impl Iterator<Item = Result<BinaryPackagePoolArtifact<'_>>> + '_ {
        self.iter_component_binary_packages(component, architecture)
            .map(|para| {
                let path = para
                    .first_field_str("Filename")
                    .expect("Filename should have been populated at package add time");
                let size = usize::from_str(
                    para.first_field_str("Size")
                        .expect("Size should have been populated at package add time"),
                )
                .expect("Size should parse to an integer");

                // Checksums are stored in a BTreeSet and sort from weakest to strongest. So use the
                // strongest available checksum.
                let strongest_checksum = self
                    .checksums
                    .iter()
                    .last()
                    .expect("should have at least 1 checksum defined");

                let digest_hex = para
                    .first_field_str(strongest_checksum.field_name())
                    .expect("checksum's field should have been set");
                let digest = ContentDigest::from_hex_checksum(*strongest_checksum, digest_hex)?;

                Ok(BinaryPackagePoolArtifact { path, size, digest })
            })
    }

    /// Obtain an [AsyncRead] that reads contents of a `Packages` file for binary packages.
    ///
    /// This is a wrapper around [Self::iter_component_binary_packages()] that normalizes the
    /// [ControlParagraph] to data and converts it to an [AsyncRead].
    pub fn component_binary_packages_reader(
        &self,
        component: impl ToString,
        architecture: impl ToString,
    ) -> impl AsyncRead + '_ {
        futures::stream::iter(
            self.iter_component_binary_packages(component, architecture)
                .map(|p| Ok(format!("{}\n", p.to_string()))),
        )
        .into_async_read()
    }

    /// Like [Self::component_binary_packages_reader()] except data is compressed.
    pub fn component_binary_packages_reader_compression(
        &self,
        component: impl ToString,
        architecture: impl ToString,
        compression: Compression,
    ) -> Pin<Box<dyn AsyncRead + Send + '_>> {
        read_compressed(
            futures::io::BufReader::new(
                self.component_binary_packages_reader(
                    component.to_string(),
                    architecture.to_string(),
                ),
            ),
            compression,
        )
    }

    /// Obtain [IndexFileReader] for each logical `Packages` file.
    pub fn binary_packages_index_readers(&self) -> impl Iterator<Item = IndexFileReader<'_>> + '_ {
        self.binary_packages
            .keys()
            .map(move |(component, architecture)| {
                self.index_file_compressions
                    .iter()
                    .map(move |compression| IndexFileReader {
                        reader: self.component_binary_packages_reader_compression(
                            component,
                            architecture,
                            *compression,
                        ),
                        compression: *compression,
                        directory: format!("{}/binary-{}", component, architecture),
                        filename: "Packages".to_string(),
                    })
            })
            .flatten()
    }

    /// Obtain all [IndexFileReader] to be published.
    ///
    /// Each item corresponds to a logical item in an `[In]Release`.
    pub fn index_file_readers(&self) -> impl Iterator<Item = IndexFileReader<'_>> + '_ {
        self.binary_packages_index_readers()
    }

    /// Obtain records describing pool artifacts needed to support binary packages.
    pub fn iter_binary_packages_pool_artifacts(
        &self,
    ) -> impl Iterator<Item = Result<BinaryPackagePoolArtifact<'_>>> + '_ {
        self.binary_packages
            .keys()
            .map(move |(component, architecture)| {
                self.iter_component_binary_package_pool_artifacts(component, architecture)
            })
            .flatten()
    }

    /// Publish artifacts to the *pool*.
    ///
    /// The *pool* is the area of a Debian repository holding files like the .deb packages.
    ///
    /// Content must be published to the pool before indices data is written, otherwise there
    /// is a race condition where the indices could refer to files not yet in the pool.
    pub async fn publish_pool_artifacts<F>(
        &self,
        resolver: &impl DataResolver,
        writer: &impl RepositoryWriter,
        threads: usize,
        progress_cb: &Option<F>,
    ) -> Result<()>
    where
        F: Fn(PublishEvent),
    {
        let artifacts = self
            .iter_binary_packages_pool_artifacts()
            .collect::<Result<Vec<_>>>()?;

        if let Some(ref cb) = progress_cb {
            cb(PublishEvent::ResolvedPoolArtifacts(artifacts.len()));
        }

        // Queue a verification check for each artifact.
        let mut fs = futures::stream::iter(
            artifacts
                .iter()
                .map(|a| writer.verify_path(a.path, Some((a.size, a.digest.clone())))),
        )
        .buffer_unordered(threads);

        let mut missing_paths = BTreeSet::new();

        while let Some(result) = fs.next().await {
            let result = result?;

            match result.state {
                RepositoryPathVerificationState::ExistsNoIntegrityCheck
                | RepositoryPathVerificationState::ExistsIntegrityVerified => {
                    if let Some(ref cb) = progress_cb {
                        cb(PublishEvent::PoolArtifactCurrent(result.path.to_string()));
                    }
                }
                RepositoryPathVerificationState::ExistsIntegrityMismatch
                | RepositoryPathVerificationState::Missing => {
                    if let Some(ref cb) = progress_cb {
                        cb(PublishEvent::PoolArtifactMissing(result.path.to_string()));
                    }

                    missing_paths.insert(result.path);
                }
            }
        }

        if let Some(ref cb) = progress_cb {
            cb(PublishEvent::PoolArtifactsToPublish(missing_paths.len()));
        }

        // Now we need to copy files from our source.

        let mut fs = futures::stream::iter(
            artifacts
                .iter()
                .filter(|a| missing_paths.contains(a.path))
                .map(|a| get_path_and_copy(resolver, writer, a)),
        )
        .buffer_unordered(threads);

        while let Some(artifact) = fs.next().await {
            let artifact = artifact?;

            if let Some(ref cb) = progress_cb {
                cb(PublishEvent::PoolArtifactCreated(
                    artifact.path.to_string(),
                    artifact.size,
                ));
            }
        }

        Ok(())
    }

    async fn expand_index_file_reader<'ifr, 'slf: 'ifr>(
        &'slf self,
        mut ifr: IndexFileReader<'ifr>,
    ) -> Result<Box<dyn Iterator<Item = ExpandedIndexFile> + 'ifr>> {
        let mut buf = vec![];
        ifr.reader.read_to_end(&mut buf).await?;

        let mut digester = MultiDigester::default();
        digester.update(&buf);
        let digests = digester.finish();

        if self.acquire_by_hash == Some(true) {
            Ok(Box::new(self.checksums.iter().map(move |checksum| {
                ExpandedIndexFile {
                    canonical_path: ifr.canonical_path(),
                    write_path: ifr.by_hash_path(digests.digest_from_checksum(*checksum)),
                    digests: digests.clone(),
                    data: buf.clone(),
                }
            })))
        } else {
            Ok(Box::new(std::iter::once(ExpandedIndexFile {
                canonical_path: ifr.canonical_path(),
                write_path: ifr.canonical_path(),
                digests,
                data: buf,
            })))
        }
    }

    /// Derive fields for `Release` files that aren't related to indices lists.
    fn static_release_fields(&self) -> impl Iterator<Item = ControlField<'_>> {
        let mut fields: BTreeMap<Cow<'_, str>, Cow<'_, str>> = BTreeMap::new();

        fields.insert(
            "Components".into(),
            self.components
                .iter()
                .map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(" ")
                .into(),
        );

        fields.insert(
            "Architectures".into(),
            self.architectures
                .iter()
                .map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(" ")
                .into(),
        );

        if let Some(suite) = &self.suite {
            fields.insert("Suite".into(), suite.into());
        }
        if let Some(codename) = &self.codename {
            fields.insert("Codename".into(), codename.into());
        }
        if let Some(date) = &self.date {
            fields.insert(
                "Date".into(),
                format!("{}", date.format(DATE_FORMAT)).into(),
            );
        }
        if let Some(valid_until) = &self.valid_until {
            fields.insert(
                "Valid-Until".into(),
                format!("{}", valid_until.format(DATE_FORMAT)).into(),
            );
        }
        if let Some(description) = &self.description {
            fields.insert("Description".into(), description.into());
        }
        if let Some(origin) = &self.origin {
            fields.insert("Origin".into(), origin.into());
        }
        if let Some(label) = &self.label {
            fields.insert("Label".into(), label.into());
        }
        if let Some(version) = &self.version {
            fields.insert("Version".into(), version.into());
        }
        if let Some(acquire_by_hash) = self.acquire_by_hash {
            fields.insert(
                "Acquire-By-Hash".into(),
                if acquire_by_hash { "yes" } else { "no" }.into(),
            );
        }

        fields
            .into_iter()
            .map(|(k, v)| ControlField::new(k, ControlFieldValue::Simple(v)))
    }

    /// Derive a [ReleaseFile] representing the content of the `Release` file.
    ///
    /// This takes an iterable describing indices files. This iterable is typically derived
    /// from [Self::index_file_readers()].
    pub fn create_release_file(
        &self,
        indices: impl Iterator<Item = (String, (u64, MultiContentDigest))>,
    ) -> Result<ReleaseFile<'_>> {
        let mut para = ControlParagraph::default();

        for field in self.static_release_fields() {
            para.add_field(field);
        }

        let mut digests_by_field = HashMap::new();

        for (path, (size, digests)) in indices {
            for digest in digests.iter_digests() {
                digests_by_field
                    .entry(digest.release_field_name())
                    .or_insert_with(BTreeMap::new)
                    .insert(path.clone(), (size, digest.digest_hex()));
            }
        }

        for checksum in self.checksums.iter() {
            let entries = digests_by_field
                .get(checksum.field_name())
                .expect("digest field should be populated");

            let longest_path = entries.keys().map(|x| x.len()).max().unwrap_or_default();
            let longest_size = entries
                .values()
                .map(|(size, _)| format!("{}", size).len())
                .max()
                .unwrap_or_default();

            para.add_field(ControlField::new(
                checksum.field_name().into(),
                ControlFieldValue::multiline_from_lines(std::iter::once("".to_string()).chain(
                    entries.iter().map(|(path, (size, digest))| {
                        format!(
                            "{:<path_width$} {:>size_width$} {}",
                            path,
                            size,
                            digest,
                            path_width = longest_path,
                            size_width = longest_size
                        )
                    }),
                )),
            ));
        }

        Ok(para.into())
    }

    /// Publish index files.
    ///
    /// Repository index files describe the contents of the repository. Index files are
    /// referred to by the `InRelease` and `Release` files.
    ///
    /// Indices should only be published after pool artifacts are published. Otherwise
    /// there is a race condition where an index file could refer to a file in the pool
    /// that does not exist.
    pub async fn publish_indices<F, PW>(
        &self,
        writer: &impl RepositoryWriter,
        path_prefix: Option<&str>,
        threads: usize,
        progress_cb: &Option<F>,
        signing_key: Option<(&impl SecretKeyTrait, PW)>,
    ) -> Result<()>
    where
        F: Fn(PublishEvent),
        PW: FnOnce() -> String,
    {
        let mut index_paths = BTreeMap::new();

        // This will effectively buffer all indices files in memory. This could be avoided if
        // we want to limit memory use.

        let mut fs = futures::stream::iter(
            self.index_file_readers()
                .map(|ifr| self.expand_index_file_reader(ifr)),
        )
        .buffer_unordered(threads);

        let mut iters = vec![];

        while let Some(res) = fs.try_next().await? {
            for mut eif in res {
                if let Some(prefix) = path_prefix {
                    eif.write_path = format!("{}/{}", prefix.trim_matches('/'), eif.write_path);
                }

                if let Some(cb) = progress_cb {
                    cb(PublishEvent::IndexFileToWrite(eif.write_path.clone()));
                }

                index_paths.insert(
                    eif.canonical_path.clone(),
                    (eif.data.len() as u64, eif.digests.clone()),
                );

                iters.push(eif);
            }
        }

        let mut fs = futures::stream::iter(iters.into_iter().map(|eif| {
            writer.write_path(
                eif.write_path.into(),
                Box::pin(futures::io::Cursor::new(eif.data)),
            )
        }))
        .buffer_unordered(threads);

        while let Some(write) = fs.try_next().await? {
            if let Some(cb) = progress_cb {
                cb(PublishEvent::IndexFileWritten(
                    write.path.to_string(),
                    write.bytes_written,
                ));
            }
        }

        // Now with all the indices files written, we can write the `[In]Release` files.

        let release = self.create_release_file(index_paths.into_iter())?;

        let (release_path, inrelease_path) = if let Some(prefix) = path_prefix {
            (
                format!("{}/Release", prefix.trim_matches('/')),
                format!("{}/InRelease", prefix.trim_matches('/')),
            )
        } else {
            ("Release".to_string(), "InRelease".to_string())
        };

        if let Some(cb) = progress_cb {
            cb(PublishEvent::IndexFileToWrite(release_path.clone()))
        }

        let release_write = writer
            .write_path(
                release_path.into(),
                Box::pin(futures::io::Cursor::new(
                    release.as_ref().to_string().into_bytes(),
                )),
            )
            .await?;

        if let Some(cb) = progress_cb {
            cb(PublishEvent::IndexFileWritten(
                release_write.path.to_string(),
                release_write.bytes_written,
            ));
        }

        if let Some((key, password)) = signing_key {
            let inrelease_content = cleartext_sign(
                key,
                password,
                HashAlgorithm::SHA2_256,
                std::io::Cursor::new(release.as_ref().to_string().as_bytes()),
            )?;

            if let Some(cb) = progress_cb {
                cb(PublishEvent::IndexFileToWrite(inrelease_path.clone()));
            }

            let inrelease_write = writer
                .write_path(
                    inrelease_path.into(),
                    Box::pin(futures::io::Cursor::new(inrelease_content.into_bytes())),
                )
                .await?;

            if let Some(cb) = progress_cb {
                cb(PublishEvent::IndexFileWritten(
                    inrelease_write.path.to_string(),
                    inrelease_write.bytes_written,
                ));
            }
        }

        Ok(())
    }

    /// Publish the repository to the given [RepositoryWriter].
    ///
    /// This is the main function for *writing out* the desired state in this builder.
    ///
    /// Publishing effectively works in 3 phases:
    ///
    /// 1. Publish missing pool artifacts.
    /// 2. Publish *indices* files (e.g. `Packages` lists).
    /// 3. Publish the `InRelease` and `Release` file.
    ///
    /// `writer` is a [RepositoryWriter] used to perform I/O for writing output files.
    /// `resolver` is a [DataResolver] for resolving pool paths. It will be consulted
    /// to obtain paths of `.deb` and other pool files.
    /// `distribution_path` is the relative path under `writer` to write indices files
    /// under. It typically begins with `dists/`. e.g. `dists/bullseye`. This value
    /// becomes the directory with the generated `InRelease` file.
    /// `threads` is the number of parallel threads to use for I/O.
    /// `progress_cb` provides an optional function to receive progress updates.
    /// `signing_key` provides a signing key for PGP signing and an optional function to
    /// obtain the password to unlock that key.
    pub async fn publish<F, PW>(
        &self,
        writer: &impl RepositoryWriter,
        resolver: &impl DataResolver,
        distribution_path: &str,
        threads: usize,
        progress_cb: &Option<F>,
        signing_key: Option<(&impl SecretKeyTrait, PW)>,
    ) -> Result<()>
    where
        F: Fn(PublishEvent),
        PW: FnOnce() -> String,
    {
        self.publish_pool_artifacts(resolver, writer, threads, progress_cb)
            .await?;

        self.publish_indices(
            writer,
            Some(distribution_path),
            threads,
            progress_cb,
            signing_key,
        )
        .await?;

        Ok(())
    }
}

async fn get_path_and_copy<'a, 'b>(
    resolver: &impl DataResolver,
    writer: &impl RepositoryWriter,
    artifact: &'a BinaryPackagePoolArtifact<'b>,
) -> Result<&'a BinaryPackagePoolArtifact<'b>> {
    // It would be slightly more defensive to plug in the content validator
    // explicitly here. However, the API contract is a contract. Let's let
    // implementations shoot themselves in the foot.
    let reader = resolver
        .get_path_with_digest_verification(artifact.path, artifact.size, artifact.digest.clone())
        .await?;

    writer.write_path(artifact.path.into(), reader).await?;

    Ok(artifact)
}

#[cfg(test)]
mod test {

    use {
        super::*,
        crate::{
            io::PathMappingDataResolver,
            repository::{
                http::HttpRepositoryClient, RepositoryPathVerification,
                RepositoryPathVerificationState, RepositoryRootReader, RepositoryWrite,
            },
            signing_key::{create_self_signed_key, signing_secret_key_params_builder},
        },
        async_trait::async_trait,
        futures::AsyncReadExt,
        std::borrow::Cow,
    };

    const BULLSEYE_URL: &str = "http://snapshot.debian.org/archive/debian/20211120T085721Z";

    #[derive(Default)]
    struct CapturingWriter {
        paths: std::sync::Mutex<HashMap<String, Vec<u8>>>,
    }

    impl CapturingWriter {
        fn get_path(&self, path: impl ToString) -> Option<Vec<u8>> {
            self.paths.lock().unwrap().get(&path.to_string()).cloned()
        }
    }

    #[async_trait]
    impl RepositoryWriter for CapturingWriter {
        async fn verify_path<'path>(
            &self,
            path: &'path str,
            _expected_content: Option<(usize, ContentDigest)>,
        ) -> Result<RepositoryPathVerification<'path>> {
            Ok(RepositoryPathVerification {
                path,
                state: RepositoryPathVerificationState::Missing,
            })
        }

        async fn write_path<'path, 'reader>(
            &self,
            path: Cow<'path, str>,
            reader: Pin<Box<dyn AsyncRead + Send + 'reader>>,
        ) -> Result<RepositoryWrite<'path>> {
            let mut writer = futures::io::Cursor::new(Vec::<u8>::new());

            let bytes_written = futures::io::copy(reader, &mut writer)
                .await
                .map_err(|e| DebianError::RepositoryIoPath(path.to_string(), e))?;

            self.paths
                .lock()
                .unwrap()
                .insert(path.to_string(), writer.into_inner());

            Ok(RepositoryWrite {
                path,
                bytes_written,
            })
        }
    }

    #[test]
    fn pool_layout_paths() {
        let layout = PoolLayout::ComponentThenNamePrefix;

        assert_eq!(
            layout.path("main", "python3.9", "python3.9_3.9.9-1_arm64.deb"),
            "pool/main/p/python3.9/python3.9_3.9.9-1_arm64.deb"
        );
        assert_eq!(
            layout.path("main", "libzstd", "zstd_1.4.8+dfsg-2.1_amd64.deb"),
            "pool/main/libz/libzstd/zstd_1.4.8+dfsg-2.1_amd64.deb"
        );
    }

    #[tokio::test]
    async fn bullseye_binary_packages_reader() -> Result<()> {
        let root = HttpRepositoryClient::new(BULLSEYE_URL).unwrap();
        let release = root.release_reader("bullseye").await.unwrap();

        let packages = release
            .resolve_packages("main", "amd64", false)
            .await
            .unwrap();

        let mut builder = RepositoryBuilder::new_recommended(
            ["all", "amd64"].iter(),
            ["main"].iter(),
            "suite",
            "codename",
        );

        let mut mapping_resolver = PathMappingDataResolver::new(root);

        // Cap total work by limiting packages examined.
        for package in packages
            .iter()
            .filter(|cf| {
                if let Some(Ok(size)) = cf.size() {
                    size < 1000000
                } else {
                    false
                }
            })
            .take(10)
        {
            let dest_filename = builder.add_binary_deb("main", package)?;

            let source_filename = package.first_field_str("Filename").unwrap();

            mapping_resolver.add_path_map(dest_filename, source_filename);
        }

        let pool_artifacts = builder
            .iter_binary_packages_pool_artifacts()
            .collect::<Result<Vec<_>>>()?;
        assert_eq!(pool_artifacts.len(), 10);

        let mut entries = builder.binary_packages_index_readers().collect::<Vec<_>>();
        assert_eq!(entries.len(), 6);
        assert!(entries
            .iter()
            .all(|entry| entry.canonical_path().starts_with("main/binary-")));

        for entry in entries.iter_mut() {
            let mut buf = vec![];
            entry.reader.read_to_end(&mut buf).await.unwrap();
        }

        let writer = CapturingWriter::default();

        let cb = |event| {
            eprintln!("{}", event);
        };

        let passwd_fn = String::new;
        let signed_secret_key = create_self_signed_key(
            signing_secret_key_params_builder("Me <someone@example.com>")
                .build()
                .unwrap(),
            passwd_fn,
        )
        .unwrap()
        .0;

        builder
            .publish(
                &writer,
                &mapping_resolver,
                "dists/mydist",
                10,
                &Some(cb),
                Some((&signed_secret_key, passwd_fn)),
            )
            .await?;

        let wanted_paths = vec!["dists/mydist/Release", "dists/mydist/InRelease"];

        assert!(wanted_paths.iter().all(|path| writer
            .paths
            .lock()
            .unwrap()
            .contains_key(&path.to_string())));

        let release = ReleaseFile::from_armored_reader(std::io::Cursor::new(
            writer.get_path("dists/mydist/InRelease").unwrap(),
        ))
        .unwrap();

        let signatures = release
            .signatures()
            .expect("PGP signatures should have been parsed");
        assert_eq!(
            signatures
                .iter_signatures_from_key(&signed_secret_key)
                .count(),
            1
        );

        signatures.verify(&signed_secret_key).unwrap();

        Ok(())
    }
}
