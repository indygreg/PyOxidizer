// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! `Release` file primitives. */

use {
    crate::{
        control::{ControlError, ControlField, ControlParagraph, ControlParagraphReader},
        pgp::MyHasher,
        repository::IndexFileCompression,
    },
    chrono::{DateTime, TimeZone, Utc},
    mailparse::{dateparse, MailParseError},
    std::{
        borrow::Cow,
        io::{BufRead, Read},
        num::ParseIntError,
        str::FromStr,
    },
    thiserror::Error,
};

/// Describes an error related to `Release` file handling.
#[derive(Debug, Error)]
pub enum ReleaseError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Date parsing error: {0}")]
    DateParse(#[from] MailParseError),

    #[error("control file error: {0}")]
    Control(#[from] ControlError),

    #[error("expected 1 paragraph in control file; got {0}")]
    ControlParagraphMismatch(usize),

    #[error("digest missing from index entry")]
    MissingDigest,

    #[error("size missing from index entry")]
    MissingSize,

    #[error("path missing from index entry")]
    MissingPath,

    #[error("error parsing size field: {0}")]
    SizeParse(#[from] ParseIntError),

    #[error("index entry path unexpectedly has spaces: {0}")]
    PathWithSpaces(String),

    #[error("No PGP signatures found")]
    NoSignatures,

    #[error("No PGP signatures found from the specified key")]
    NoSignaturesByKey,

    #[error("invalid hexadecimal in content digest: {0:?}")]
    FromHex(#[from] hex::FromHexError),
}

/// Checksum type / digest mechanism used in a release file.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ChecksumType {
    /// MD5.
    Md5,

    /// SHA-1.
    Sha1,

    /// SHA-256.
    Sha256,
}

impl ChecksumType {
    /// Name of the control field in `Release` files holding this variant type.
    pub fn field_name(&self) -> &'static str {
        match self {
            Self::Md5 => "MD5Sum",
            Self::Sha1 => "SHA1",
            Self::Sha256 => "SHA256",
        }
    }

    /// Obtain a new hasher for this checksum flavor.
    pub fn new_hasher(&self) -> Box<dyn pgp::crypto::Hasher + Send> {
        Box::new(match self {
            Self::Md5 => MyHasher::md5(),
            Self::Sha1 => MyHasher::sha1(),
            Self::Sha256 => MyHasher::sha256(),
        })
    }
}

/// A typed digest in a release file.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ReleaseFileDigest<'a> {
    Md5(&'a str),
    Sha1(&'a str),
    Sha256(&'a str),
}

impl<'a> ReleaseFileDigest<'a> {
    /// Create a new instance given the checksum type and a digest value.
    pub fn new(checksum: ChecksumType, value: &'a str) -> Self {
        match checksum {
            ChecksumType::Md5 => Self::Md5(value),
            ChecksumType::Sha1 => Self::Sha1(value),
            ChecksumType::Sha256 => Self::Sha256(value),
        }
    }

    /// The name of the `Release` paragraph field from which this digest came.
    ///
    /// Is also the `by-hash` path component.
    pub fn field_name(&self) -> &'static str {
        match self {
            Self::Md5(_) => ChecksumType::Md5.field_name(),
            Self::Sha1(_) => ChecksumType::Sha1.field_name(),
            Self::Sha256(_) => ChecksumType::Sha256.field_name(),
        }
    }

    /// Obtain the tracked digest value.
    pub fn digest(&self) -> &'a str {
        match self {
            Self::Md5(v) => v,
            Self::Sha1(v) => v,
            Self::Sha256(v) => v,
        }
    }
}

/// An entry for a file in a parsed `Release` file.
///
/// Instances correspond to a line in a `MD5Sum`, `SHA1`, or `SHA256` field.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct ReleaseFileEntry<'a> {
    /// The path to this file within the repository.
    pub path: &'a str,

    /// The hex digest of this file.
    pub digest: ReleaseFileDigest<'a>,

    /// The size of the file in bytes.
    pub size: usize,
}

impl<'a> ReleaseFileEntry<'a> {
    /// Obtain the `by-hash` path variant for this entry.
    pub fn by_hash_path(&self) -> String {
        if let Some((prefix, _)) = self.path.rsplit_once('/') {
            format!(
                "{}/by-hash/{}/{}",
                prefix,
                self.digest.field_name(),
                self.digest.digest()
            )
        } else {
            format!(
                "by-hash/{}/{}",
                self.digest.field_name(),
                self.digest.digest()
            )
        }
    }

    /// Obtain the content digest as bytes.
    pub fn digest_bytes(&self) -> Result<Vec<u8>, ReleaseError> {
        Ok(hex::decode(self.digest.digest())?)
    }
}

/// A type of [ReleaseFileEntry] that describes a `Contents` file.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentsFileEntry<'a> {
    /// The [ReleaseFileEntry] from which this instance was derived.
    pub entry: ReleaseFileEntry<'a>,

    /// The parsed component name (from the entry's path).
    pub component: Cow<'a, str>,

    /// The parsed architecture name (from the entry's path).
    pub architecture: Cow<'a, str>,

    /// File-level compression format being used.
    pub compression: IndexFileCompression,

    /// Whether this refers to udeb packages used by installers.
    pub is_installer: bool,
}

/// A special type of [ReleaseFileEntry] that describes a `Packages` file.
#[derive(Clone, Debug, PartialEq)]
pub struct PackagesFileEntry<'a> {
    /// The [ReleaseFileEntry] from which this instance was derived.
    pub entry: ReleaseFileEntry<'a>,

    /// The parsed component name (from the entry's path).
    pub component: Cow<'a, str>,

    /// The parsed architecture name (from the entry's path).
    pub architecture: Cow<'a, str>,

    /// File-level compression format being used.
    pub compression: IndexFileCompression,

    /// Whether this refers to udeb packages used by installers.
    pub is_installer: bool,
}

impl<'a> ReleaseFileEntry<'a> {
    /// Attempt to convert this instance to a [ContentsFileEntry].
    ///
    /// Resolves to [Some] if the conversion succeeded or [None] if this (likely)
    /// isn't a `Contents*` file.
    pub fn to_contents_file_entry(self) -> Option<ContentsFileEntry<'a>> {
        let parts = self.path.split('/').collect::<Vec<_>>();

        let filename = *parts.last()?;

        let suffix = filename.strip_prefix("Contents-")?;

        let (architecture, compression) = if let Some(v) = suffix.strip_suffix(".gz") {
            (v, IndexFileCompression::Gzip)
        } else {
            (suffix, IndexFileCompression::None)
        };

        let (architecture, is_installer) = if let Some(v) = architecture.strip_prefix("udeb-") {
            (v, true)
        } else {
            (architecture, false)
        };

        // The component is the part up until the `/Contents*` final path component.
        let component = &self.path[..self.path.len() - filename.len() - 1];

        Some(ContentsFileEntry {
            entry: self,
            component: component.into(),
            architecture: architecture.into(),
            compression,
            is_installer,
        })
    }

    /// Attempt to convert this instance to a [PackagesFileEntry].
    ///
    /// Resolves to [Some] if the conversion succeeded or [None] if this (likely)
    /// isn't a `Packages*` file.
    pub fn to_packages_file_entry(self) -> Option<PackagesFileEntry<'a>> {
        let parts = self.path.split('/').collect::<Vec<_>>();

        let compression = match *parts.last()? {
            "Packages" => IndexFileCompression::None,
            "Packages.xz" => IndexFileCompression::Xz,
            "Packages.gz" => IndexFileCompression::Gzip,
            "Packages.bz2" => IndexFileCompression::Bzip2,
            "Packages.lzma" => IndexFileCompression::Lzma,
            _ => {
                return None;
            }
        };

        // The component and architecture are the directory components before the
        // filename. The architecture is limited to a single directory component but
        // the component can have multiple directories.

        let architecture_component = *parts.iter().rev().nth(1)?;

        let search = &self.path[..self.path.len() - parts.last()?.len() - 1];
        let component = &search[0..search.rfind('/')?];

        // The architecture part is prefixed with `binary-`.
        let architecture = architecture_component.strip_prefix("binary-")?;

        // udeps have a `debian-installer` path component following the component.
        let (component, is_udeb) =
            if let Some(component) = component.strip_suffix("/debian-installer") {
                (component, true)
            } else {
                (component, false)
            };

        Some(PackagesFileEntry {
            entry: self,
            component: component.into(),
            architecture: architecture.into(),
            compression,
            is_installer: is_udeb,
        })
    }
}

/// A Debian repository `Release` file.
///
/// Release files contain metadata and list the index files for a *repository*.
///
/// Instances are wrappers around a [ControlParagraph]. [AsRef] and [AsMut] are
/// implemented to allow obtaining the inner [ControlParagraph].
pub struct ReleaseFile<'a> {
    paragraph: ControlParagraph<'a>,

    /// Parsed PGP signatures for this file.
    signatures: Option<crate::pgp::CleartextSignatures>,
}

impl<'a> ReleaseFile<'a> {
    /// Construct an instance by reading data from a reader.
    ///
    /// The source must be a Debian control file with exactly 1 paragraph.
    ///
    /// The source must not be PGP armored. i.e. do not feed it raw `InRelease`
    /// files that begin with `-----BEGIN PGP SIGNED MESSAGE-----`.
    pub fn from_reader<R: BufRead>(reader: R) -> Result<Self, ReleaseError> {
        let paragraphs = ControlParagraphReader::new(reader).collect::<Result<Vec<_>, _>>()?;

        // A Release control file should have a single paragraph.

        // A Release control file should have a single paragraph.
        if paragraphs.len() != 1 {
            return Err(ReleaseError::ControlParagraphMismatch(paragraphs.len()));
        }

        let paragraph = paragraphs
            .into_iter()
            .next()
            .expect("validated paragraph count above");

        Ok(Self {
            paragraph,
            signatures: None,
        })
    }

    /// Construct an instance by reading data from a reader containing a PGP cleartext signature.
    ///
    /// This can be used to parse content from an `InRelease` file, which begins
    /// with `-----BEGIN PGP SIGNED MESSAGE-----`.
    ///
    /// An error occurs if the PGP cleartext file is not well-formed or if a PGP parsing
    /// error occurs.
    ///
    /// The PGP signature is NOT validated. The file will be parsed despite lack of
    /// signature verification. This is conceptually insecure. But since Rust has memory
    /// safety, some risk is prevented.
    pub fn from_armored_reader<R: Read>(reader: R) -> Result<Self, ReleaseError> {
        let reader = crate::pgp::CleartextSignatureReader::new(reader);
        let mut reader = std::io::BufReader::new(reader);

        let mut slf = Self::from_reader(&mut reader)?;
        slf.signatures = Some(reader.into_inner().finalize());

        Ok(slf)
    }

    /// Obtain the first occurrence of the given field.
    pub fn first_field(&self, name: &str) -> Option<&ControlField<'_>> {
        self.paragraph.first_field(name)
    }

    /// Obtain the first value of a field, evaluated as a boolean.
    ///
    /// The field is [true] iff its string value is `yes`.
    pub fn first_field_bool(&self, name: &str) -> Option<bool> {
        self.paragraph
            .first_field_str(name)
            .map(|v| matches!(v, "yes"))
    }

    /// Description of this repository.
    pub fn description(&self) -> Option<&str> {
        self.paragraph.first_field_str("Description")
    }

    /// Origin of the repository.
    pub fn origin(&self) -> Option<&str> {
        self.paragraph.first_field_str("Origin")
    }

    /// Label for the repository.
    pub fn label(&self) -> Option<&str> {
        self.paragraph.first_field_str("Label")
    }

    /// Version of this repository.
    ///
    /// Typically a sequence of `.` delimited integers.
    pub fn version(&self) -> Option<&str> {
        self.paragraph.first_field_str("Version")
    }

    /// Suite of this repository.
    ///
    /// e.g. `stable`, `unstable`, `experimental`.
    pub fn suite(&self) -> Option<&str> {
        self.paragraph.first_field_str("Suite")
    }

    /// Codename of this repository.
    pub fn codename(&self) -> Option<&str> {
        self.paragraph.first_field_str("Codename")
    }

    /// Names of components within this repository.
    ///
    /// These are areas within the repository. Values may contain path characters.
    /// e.g. `main`, `updates/main`.
    pub fn components(&self) -> Option<Box<(dyn Iterator<Item = &str> + '_)>> {
        self.paragraph.first_field_iter_value_words("Components")
    }

    /// Debian machine architectures supported by this repository.
    ///
    /// e.g. `all`, `amd64`, `arm64`.
    pub fn architectures(&self) -> Option<Box<(dyn Iterator<Item = &str> + '_)>> {
        self.paragraph.first_field_iter_value_words("Architectures")
    }

    /// Time the release file was created, as its raw string value.
    pub fn date_str(&self) -> Option<&str> {
        self.paragraph.first_field_str("Date")
    }

    /// Time the release file was created, as a [DateTime].
    ///
    /// The timezone from the original file is always normalized to UTC.
    pub fn date(&self) -> Option<Result<DateTime<Utc>, ReleaseError>> {
        self.date_str().map(|v| Ok(Utc.timestamp(dateparse(v)?, 0)))
    }

    /// Time the release file should be considered expired by the client, as its raw string value.
    pub fn valid_until_str(&self) -> Option<&str> {
        self.paragraph.first_field_str("Valid-Until")
    }

    /// Time the release file should be considered expired by the client.
    pub fn valid_until(&self) -> Option<Result<DateTime<Utc>, ReleaseError>> {
        self.valid_until_str()
            .map(|v| Ok(Utc.timestamp(dateparse(v)?, 0)))
    }

    /// Evaluated value for `NotAutomatic` field.
    ///
    /// `true` is returned iff the value is `yes`. `no` and other values result in `false`.
    pub fn not_automatic(&self) -> Option<bool> {
        self.first_field_bool("NotAutomatic")
    }

    /// Evaluated value for `ButAutomaticUpgrades` field.
    ///
    /// `true` is returned iff the value is `yes`. `no` and other values result in `false`.
    pub fn but_automatic_upgrades(&self) -> Option<bool> {
        self.first_field_bool("ButAutomaticUpgrades")
    }

    /// Whether to acquire files by hash.
    pub fn acquire_by_hash(&self) -> Option<bool> {
        self.first_field_bool("Acquire-By-Hash")
    }

    /// Obtain indexed files in this repository.
    ///
    /// Files are grouped by their checksum variant.
    ///
    /// If the specified checksum variant is present, [Some] is returned.
    ///
    /// The returned iterator emits [ReleaseFileEntry] instances. Entries are lazily
    /// parsed and parse failures will result in an error.
    pub fn iter_index_files(
        &self,
        checksum: ChecksumType,
    ) -> Option<Box<(dyn Iterator<Item = Result<ReleaseFileEntry, ReleaseError>> + '_)>> {
        if let Some(iter) = self
            .paragraph
            .first_field_iter_values(checksum.field_name())
        {
            Some(Box::new(iter.map(move |v| {
                // Values are of form: <digest> <size> <path>

                let mut parts = v.split_ascii_whitespace();

                let digest = parts.next().ok_or(ReleaseError::MissingDigest)?;
                let size = parts.next().ok_or(ReleaseError::MissingSize)?;
                let path = parts.next().ok_or(ReleaseError::MissingPath)?;

                // Are paths with spaces allowed?
                if parts.next().is_some() {
                    return Err(ReleaseError::PathWithSpaces(v.to_string()));
                }

                let digest = ReleaseFileDigest::new(checksum, digest);
                let size = usize::from_str(size)?;

                Ok(ReleaseFileEntry { path, digest, size })
            })))
        } else {
            None
        }
    }

    /// Obtain `Contents` indices entries given a checksum flavor.
    ///
    /// This essentially looks for `Contents*` files in the file lists.
    ///
    /// The emitted entries have component and architecture values derived by the
    /// file paths. These values are not checked against the list of components
    /// and architectures defined by this file.
    pub fn iter_contents_indices(
        &self,
        checksum: ChecksumType,
    ) -> Option<Box<(dyn Iterator<Item = Result<ContentsFileEntry, ReleaseError>> + '_)>> {
        if let Some(iter) = self.iter_index_files(checksum) {
            Some(Box::new(iter.filter_map(|entry| match entry {
                Ok(entry) => entry.to_contents_file_entry().map(Ok),
                Err(e) => Some(Err(e)),
            })))
        } else {
            None
        }
    }

    /// Obtain `Packages` indices entries given a checksum flavor.
    ///
    /// This essentially looks for `Packages*` files in the file lists.
    ///
    /// The emitted entries have component and architecture values derived by the
    /// file paths. These values are not checked against the list of components
    /// and architectures defined by this file.
    pub fn iter_packages_indices(
        &self,
        checksum: ChecksumType,
    ) -> Option<Box<(dyn Iterator<Item = Result<PackagesFileEntry, ReleaseError>> + '_)>> {
        if let Some(iter) = self.iter_index_files(checksum) {
            Some(Box::new(iter.filter_map(|entry| match entry {
                Ok(entry) => entry.to_packages_file_entry().map(Ok),
                Err(e) => Some(Err(e)),
            })))
        } else {
            None
        }
    }

    /// Find a [PackagesFileEntry] given search constraints.
    pub fn find_packages_indices(
        &self,
        checksum: ChecksumType,
        compression: IndexFileCompression,
        component: &str,
        arch: &str,
        is_installer: bool,
    ) -> Option<PackagesFileEntry> {
        if let Some(mut iter) = self.iter_packages_indices(checksum) {
            iter.find_map(|entry| {
                if let Ok(entry) = entry {
                    if entry.component == component
                        && entry.architecture == arch
                        && entry.is_installer == is_installer
                        && entry.compression == compression
                    {
                        Some(entry)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        } else {
            None
        }
    }
}

impl<'a> AsRef<ControlParagraph<'a>> for ReleaseFile<'a> {
    fn as_ref(&self) -> &ControlParagraph<'a> {
        &self.paragraph
    }
}

impl<'a> AsMut<ControlParagraph<'a>> for ReleaseFile<'a> {
    fn as_mut(&mut self) -> &mut ControlParagraph<'a> {
        &mut self.paragraph
    }
}

#[cfg(test)]
mod test {
    use {super::*, pgp::Deserializable};

    #[test]
    fn parse_bullseye_release() -> Result<(), ReleaseError> {
        let mut reader =
            std::io::Cursor::new(include_bytes!("../testdata/release-debian-bullseye"));

        let release = ReleaseFile::from_reader(&mut reader)?;

        assert_eq!(
            release.description(),
            Some("Debian 11.1 Released 09 October 2021")
        );
        assert_eq!(release.origin(), Some("Debian"));
        assert_eq!(release.label(), Some("Debian"));
        assert_eq!(release.version(), Some("11.1"));
        assert_eq!(release.suite(), Some("stable"));
        assert_eq!(release.codename(), Some("bullseye"));
        assert_eq!(
            release.components().unwrap().collect::<Vec<_>>(),
            vec!["main", "contrib", "non-free"]
        );
        assert_eq!(
            release.architectures().unwrap().collect::<Vec<_>>(),
            vec![
                "all", "amd64", "arm64", "armel", "armhf", "i386", "mips64el", "mipsel", "ppc64el",
                "s390x"
            ]
        );
        assert_eq!(release.date_str(), Some("Sat, 09 Oct 2021 09:34:56 UTC"));
        assert_eq!(
            release.date().unwrap()?,
            DateTime::<Utc>::from_utc(
                chrono::NaiveDateTime::new(
                    chrono::NaiveDate::from_ymd(2021, 10, 9),
                    chrono::NaiveTime::from_hms(9, 34, 56)
                ),
                Utc
            )
        );

        assert!(release.valid_until_str().is_none());

        let entries = release
            .iter_index_files(ChecksumType::Md5)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(entries.len(), 600);
        assert_eq!(
            entries[0],
            ReleaseFileEntry {
                path: "contrib/Contents-all",
                digest: ReleaseFileDigest::Md5("7fdf4db15250af5368cc52a91e8edbce"),
                size: 738242,
            }
        );
        assert_eq!(
            entries[0].by_hash_path(),
            "contrib/by-hash/MD5Sum/7fdf4db15250af5368cc52a91e8edbce"
        );
        assert_eq!(
            entries[1],
            ReleaseFileEntry {
                path: "contrib/Contents-all.gz",
                digest: ReleaseFileDigest::Md5("cbd7bc4d3eb517ac2b22f929dfc07b47"),
                size: 57319,
            }
        );
        assert_eq!(
            entries[1].by_hash_path(),
            "contrib/by-hash/MD5Sum/cbd7bc4d3eb517ac2b22f929dfc07b47"
        );
        assert_eq!(
            entries[599],
            ReleaseFileEntry {
                path: "non-free/source/Sources.xz",
                digest: ReleaseFileDigest::Md5("e3830f6fc5a946b5a5b46e8277e1d86f"),
                size: 80488,
            }
        );
        assert_eq!(
            entries[599].by_hash_path(),
            "non-free/source/by-hash/MD5Sum/e3830f6fc5a946b5a5b46e8277e1d86f"
        );

        assert!(release.iter_index_files(ChecksumType::Sha1).is_none());

        let entries = release
            .iter_index_files(ChecksumType::Sha256)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(entries.len(), 600);
        assert_eq!(
            entries[0],
            ReleaseFileEntry {
                path: "contrib/Contents-all",
                digest: ReleaseFileDigest::Sha256(
                    "3957f28db16e3f28c7b34ae84f1c929c567de6970f3f1b95dac9b498dd80fe63"
                ),
                size: 738242,
            }
        );
        assert_eq!(entries[0].by_hash_path(), "contrib/by-hash/SHA256/3957f28db16e3f28c7b34ae84f1c929c567de6970f3f1b95dac9b498dd80fe63");
        assert_eq!(
            entries[1],
            ReleaseFileEntry {
                path: "contrib/Contents-all.gz",
                digest: ReleaseFileDigest::Sha256(
                    "3e9a121d599b56c08bc8f144e4830807c77c29d7114316d6984ba54695d3db7b"
                ),
                size: 57319,
            }
        );
        assert_eq!(entries[1].by_hash_path(), "contrib/by-hash/SHA256/3e9a121d599b56c08bc8f144e4830807c77c29d7114316d6984ba54695d3db7b");
        assert_eq!(
            entries[599],
            ReleaseFileEntry {
                digest: ReleaseFileDigest::Sha256(
                    "30f3f996941badb983141e3b29b2ed5941d28cf81f9b5f600bb48f782d386fc7"
                ),
                size: 80488,
                path: "non-free/source/Sources.xz",
            }
        );
        assert_eq!(entries[599].by_hash_path(), "non-free/source/by-hash/SHA256/30f3f996941badb983141e3b29b2ed5941d28cf81f9b5f600bb48f782d386fc7");

        let contents = release
            .iter_contents_indices(ChecksumType::Sha256)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(contents.len(), 126);

        assert_eq!(
            contents[0],
            ContentsFileEntry {
                entry: ReleaseFileEntry {
                    path: "contrib/Contents-all",
                    digest: ReleaseFileDigest::Sha256(
                        "3957f28db16e3f28c7b34ae84f1c929c567de6970f3f1b95dac9b498dd80fe63"
                    ),
                    size: 738242,
                },
                component: "contrib".into(),
                architecture: "all".into(),
                compression: IndexFileCompression::None,
                is_installer: false
            }
        );
        assert_eq!(
            contents[1],
            ContentsFileEntry {
                entry: ReleaseFileEntry {
                    path: "contrib/Contents-all.gz",
                    digest: ReleaseFileDigest::Sha256(
                        "3e9a121d599b56c08bc8f144e4830807c77c29d7114316d6984ba54695d3db7b"
                    ),
                    size: 57319,
                },
                component: "contrib".into(),
                architecture: "all".into(),
                compression: IndexFileCompression::Gzip,
                is_installer: false
            }
        );
        assert_eq!(
            contents[24],
            ContentsFileEntry {
                entry: ReleaseFileEntry {
                    path: "contrib/Contents-udeb-amd64",
                    digest: ReleaseFileDigest::Sha256(
                        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    ),
                    size: 0,
                },
                component: "contrib".into(),
                architecture: "amd64".into(),
                compression: IndexFileCompression::None,
                is_installer: true
            }
        );

        let packages = release
            .iter_packages_indices(ChecksumType::Sha256)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(packages.len(), 180);

        assert_eq!(
            packages[0],
            PackagesFileEntry {
                entry: ReleaseFileEntry {
                    path: "contrib/binary-all/Packages",
                    digest: ReleaseFileDigest::Sha256(
                        "48cfe101cd84f16baf720b99e8f2ff89fd7e063553966d8536b472677acb82f0"
                    ),
                    size: 103223,
                },
                component: "contrib".into(),
                architecture: "all".into(),
                compression: IndexFileCompression::None,
                is_installer: false
            }
        );
        assert_eq!(
            packages[1],
            PackagesFileEntry {
                entry: ReleaseFileEntry {
                    path: "contrib/binary-all/Packages.gz",
                    digest: ReleaseFileDigest::Sha256(
                        "86057fcd3eff667ec8e3fbabb2a75e229f5e99f39ace67ff0db4a8509d0707e4"
                    ),
                    size: 27334,
                },
                component: "contrib".into(),
                architecture: "all".into(),
                compression: IndexFileCompression::Gzip,
                is_installer: false
            }
        );
        assert_eq!(
            packages[2],
            PackagesFileEntry {
                entry: ReleaseFileEntry {
                    path: "contrib/binary-all/Packages.xz",
                    digest: ReleaseFileDigest::Sha256(
                        "706c840235798e098d4d6013d1dabbc967f894d0ffa02c92ac959dcea85ddf54"
                    ),
                    size: 23912,
                },
                component: "contrib".into(),
                architecture: "all".into(),
                compression: IndexFileCompression::Xz,
                is_installer: false
            }
        );

        let udeps = packages
            .into_iter()
            .filter(|x| x.is_installer)
            .collect::<Vec<_>>();

        assert_eq!(udeps.len(), 90);
        assert_eq!(
            udeps[0],
            PackagesFileEntry {
                entry: ReleaseFileEntry {
                    path: "contrib/debian-installer/binary-all/Packages",
                    digest: ReleaseFileDigest::Sha256(
                        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    ),
                    size: 0,
                },
                component: "contrib".into(),
                architecture: "all".into(),
                compression: IndexFileCompression::None,
                is_installer: true
            }
        );

        Ok(())
    }

    fn bullseye_signing_key() -> pgp::SignedPublicKey {
        pgp::SignedPublicKey::from_armor_single(std::io::Cursor::new(include_bytes!(
            "../testdata/release-key-bullseye.asc"
        )))
        .unwrap()
        .0
    }

    #[test]
    fn parse_bullseye_inrelease() -> Result<(), ReleaseError> {
        let reader = std::io::Cursor::new(include_bytes!("../testdata/inrelease-debian-bullseye"));

        let release = ReleaseFile::from_armored_reader(reader)?;

        let signing_key = bullseye_signing_key();

        assert_eq!(release.signatures.unwrap().verify(&signing_key).unwrap(), 1);

        Ok(())
    }

    #[test]
    fn bad_signature_rejection() -> Result<(), ReleaseError> {
        let reader = std::io::Cursor::new(
            include_str!("../testdata/inrelease-debian-bullseye").replace(
                "d41d8cd98f00b204e9800998ecf8427e",
                "d41d8cd98f00b204e9800998ecf80000",
            ),
        );
        let release = ReleaseFile::from_armored_reader(reader)?;

        let signing_key = bullseye_signing_key();

        assert!(release.signatures.unwrap().verify(&signing_key).is_err());

        Ok(())
    }
}
