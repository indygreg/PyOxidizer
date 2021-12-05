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
        binary_package_control::BinaryPackageControlError,
        control::{ControlError, ControlParagraph},
        deb::{DebError, DebPackageReference},
        io::read_compressed,
        repository::{release::ChecksumType, Compression},
    },
    chrono::{DateTime, Utc},
    futures::{AsyncRead, TryStreamExt},
    std::collections::{BTreeMap, BTreeSet},
    thiserror::Error,
};

/// Error related to creating Debian package repositories.
#[derive(Debug, Error)]
pub enum RepositoryBuilderError {
    #[error("control file error: {0:?}")]
    Control(#[from] ControlError),

    #[error("binary package control file error: {0:?}")]
    BinaryPackageControl(#[from] BinaryPackageControlError),

    #[error("deb file handling error: {0:?}")]
    Deb(#[from] DebError),

    #[error("attempting to add package to undefined component: {0}")]
    UnknownComponent(String),

    #[error("attempting to add package to undefined architecture: {0}")]
    UnknownArchitecture(String),

    #[error("pool layout cannot be changed after content is indexed")]
    PoolLayoutImmutable,
}

/// Result type having [RepositoryBuilderError].
pub type Result<T> = std::result::Result<T, RepositoryBuilderError>;

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

/// Describes a `Packages` index file and provides a source of its content.
pub struct BinaryPackagesIndexReader<'a> {
    pub component: &'a str,
    pub architecture: &'a str,
    pub compression: Compression,
    pub reader: Box<dyn AsyncRead + Unpin + 'a>,
}

impl<'a> BinaryPackagesIndexReader<'a> {
    /// Obtain the path of this entry, relative to the `Release` file.
    pub fn path(&self) -> String {
        format!(
            "{}/binary-{}/Packages{}",
            self.component,
            self.architecture,
            self.compression.extension()
        )
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
/// [Self::add_deb()].
#[derive(Debug, Default)]
pub struct RepositoryBuilder {
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
    binary_packages: ComponentBinaryPackages<'static>,
    installer_packages: ComponentBinaryPackages<'static>,
    source_packages: BTreeMap<String, IndexedBinaryPackages<'static>>,
    translations: BTreeMap<String, ()>,
}

impl RepositoryBuilder {
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
            Err(RepositoryBuilderError::PoolLayoutImmutable)
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
    /// The specified [component] name must be registered with this instance or an error will
    /// occur.
    pub fn add_binary_deb(&mut self, component: &str, deb: impl DebPackageReference) -> Result<()> {
        if !self.components.contains(component) {
            return Err(RepositoryBuilderError::UnknownComponent(
                component.to_string(),
            ));
        }

        let original_control_file = deb.control_file()?;

        let package = original_control_file.package()?;
        let version = original_control_file.version_str()?;
        let arch = original_control_file.architecture()?;

        if !self.architectures.contains(arch) {
            return Err(RepositoryBuilderError::UnknownArchitecture(
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
        let filename = self.pool_layout.path(component, package, &deb.filename());
        para.add_field_from_string("Filename".into(), filename.into())?;

        // `Size` shouldn't be in the original control file, since it is a property of the
        // `.deb` in which the control file is embedded.
        para.add_field_from_string("Size".into(), format!("{}", deb.size_bytes()).into())?;

        // Add all configured digests for this repository.
        for checksum in &self.checksums {
            let digest = deb.digest(*checksum)?;
            let hex_digest = hex::encode(&digest);

            para.add_field_from_string(checksum.field_name().into(), hex_digest.into())?;
        }

        let component_key = (component.to_string(), arch.to_string());
        let package_key = (package.to_string(), version.to_string());
        self.binary_packages
            .entry(component_key)
            .or_default()
            .insert(package_key, para);

        Ok(())
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
                .map(|p| Ok(p.to_string())),
        )
        .into_async_read()
    }

    /// Like [Self::component_binary_packages_reader()] except data is compressed.
    pub fn component_binary_packages_reader_compression(
        &self,
        component: impl ToString,
        architecture: impl ToString,
        compression: Compression,
    ) -> Box<dyn AsyncRead + Unpin + '_> {
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

    /// Obtain a descriptor of each `Packages` file to write.
    pub fn binary_packages_indices(
        &self,
    ) -> impl Iterator<Item = BinaryPackagesIndexReader<'_>> + '_ {
        self.binary_packages
            .keys()
            .map(move |(component, architecture)| {
                self.index_file_compressions.iter().map(move |compression| {
                    BinaryPackagesIndexReader {
                        component,
                        architecture,
                        compression: *compression,
                        reader: self.component_binary_packages_reader_compression(
                            component,
                            architecture,
                            *compression,
                        ),
                    }
                })
            })
            .flatten()
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
        crate::{
            deb::InMemoryDebFile,
            repository::{http::HttpRepositoryClient, RepositoryRootReader},
        },
        futures::AsyncReadExt,
    };

    const BULLSEYE_URL: &str = "http://snapshot.debian.org/archive/debian/20211120T085721Z";

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

        let git_packages = packages
            .find_packages_with_name("git".into())
            .collect::<Vec<_>>();
        assert_eq!(git_packages.len(), 1);
        let git = git_packages[0];

        let git_pool_path = git.first_field_str("Filename").unwrap();

        let mut deb_data = vec![];
        root.get_path(git_pool_path)
            .await
            .unwrap()
            .read_to_end(&mut deb_data)
            .await
            .unwrap();

        let git_filename = git
            .first_field_str("Filename")
            .unwrap()
            .rsplit_once('/')
            .unwrap()
            .1;

        let deb_package = InMemoryDebFile::new(git_filename.into(), deb_data);

        let mut builder = RepositoryBuilder::new_recommended(
            ["amd64"].iter(),
            ["main"].iter(),
            "suite",
            "codename",
        );
        builder.add_binary_deb("main", deb_package)?;

        let mut packages_data = vec![];
        builder
            .component_binary_packages_reader("main", "amd64")
            .read_to_end(&mut packages_data)
            .await
            .unwrap();

        let mut entries = builder.binary_packages_indices().collect::<Vec<_>>();
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().all(|entry| entry.component == "main"));
        assert!(entries.iter().all(|entry| entry.architecture == "amd64"));

        for entry in entries.iter_mut() {
            let mut buf = vec![];
            entry.reader.read_to_end(&mut buf).await.unwrap();
        }

        Ok(())
    }
}
