// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian source control files. */

use {
    crate::{
        control::{ControlParagraph, ControlParagraphReader},
        dependency::{DependencyList, PackageDependencyFields},
        error::{DebianError, Result},
        io::ContentDigest,
        package_version::PackageVersion,
        repository::release::ChecksumType,
    },
    std::{
        io::{BufRead, Read},
        ops::{Deref, DerefMut},
        str::FromStr,
    },
};

/// A single file as described by a `Files` or `Checksums-*` field in a [DebianSourceControlFile].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebianSourceControlFileEntry<'a> {
    /// The filename/path.
    pub filename: &'a str,

    /// The content digest of this file.
    pub digest: ContentDigest,

    /// The size in bytes of the file.
    pub size: u64,
}

/// Describes a single binary package entry in a `Package-List` field in a [DebianSourceControlFile].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebianSourceControlFilePackage<'a> {
    /// The name of the binary package.
    pub name: &'a str,
    /// The package type.
    pub package_type: &'a str,
    /// The section it appears in.
    pub section: &'a str,
    /// The package priority.
    pub priority: &'a str,
    /// Extra fields.
    pub extra: Vec<&'a str>,
}

/// A Debian source control file/paragraph.
///
/// This control file consists of a single paragraph and defines a source package.
/// This paragraph is typically found in `.dsc` files and in `Sources` files in repositories.
///
/// The fields are defined at
/// <https://www.debian.org/doc/debian-policy/ch-controlfields.html#debian-source-control-files-dsc>.
#[derive(Default)]
pub struct DebianSourceControlFile<'a> {
    paragraph: ControlParagraph<'a>,
    /// Parsed PGP signatures for this file.
    signatures: Option<crate::pgp::CleartextSignatures>,
}

impl<'a> Deref for DebianSourceControlFile<'a> {
    type Target = ControlParagraph<'a>;

    fn deref(&self) -> &Self::Target {
        &self.paragraph
    }
}

impl<'a> DerefMut for DebianSourceControlFile<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.paragraph
    }
}

impl<'a> From<ControlParagraph<'a>> for DebianSourceControlFile<'a> {
    fn from(paragraph: ControlParagraph<'a>) -> Self {
        Self {
            paragraph,
            signatures: None,
        }
    }
}

impl<'a> From<DebianSourceControlFile<'a>> for ControlParagraph<'a> {
    fn from(cf: DebianSourceControlFile<'a>) -> Self {
        cf.paragraph
    }
}

impl<'a> DebianSourceControlFile<'a> {
    /// Construct an instance by reading data from a reader.
    ///
    /// The source must be a Debian source control file with exactly 1 paragraph.
    ///
    /// The source must not be PGP armored (e.g. beginning with
    /// `-----BEGIN PGP SIGNED MESSAGE-----`). For PGP armored data, use
    /// [Self::from_armored_reader()].
    pub fn from_reader<R: BufRead>(reader: R) -> Result<Self> {
        let paragraphs = ControlParagraphReader::new(reader).collect::<Result<Vec<_>>>()?;

        if paragraphs.len() != 1 {
            return Err(DebianError::DebianSourceControlFileParagraphMismatch(
                paragraphs.len(),
            ));
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
    /// This can be used to parse content from a `.dsc` file which begins
    /// with `-----BEGIN PGP SIGNED MESSAGE-----`.
    ///
    /// An error occurs if the PGP cleartext file is not well-formed or if a PGP parsing
    /// error occurs.
    ///
    /// The PGP signature is NOT validated. The file will be parsed despite lack of
    /// signature verification. This is conceptually insecure. But since Rust has memory
    /// safety, some risk is prevented.
    pub fn from_armored_reader<R: Read>(reader: R) -> Result<Self> {
        let reader = crate::pgp::CleartextSignatureReader::new(reader);
        let mut reader = std::io::BufReader::new(reader);

        let mut slf = Self::from_reader(&mut reader)?;
        slf.signatures = Some(reader.into_inner().finalize());

        Ok(slf)
    }

    /// Obtain PGP signatures from this possibly signed file.
    pub fn signatures(&self) -> Option<&crate::pgp::CleartextSignatures> {
        self.signatures.as_ref()
    }

    /// The format of the source package.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-format>.
    pub fn format(&self) -> Result<&str> {
        self.required_field_str("Format")
    }

    /// The name of the source package.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-source>.
    pub fn source(&self) -> Result<&str> {
        self.required_field_str("Source")
    }

    /// The binary packages this source package produces.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-binary>.
    pub fn binary(&self) -> Option<Box<(dyn Iterator<Item = &str> + '_)>> {
        self.iter_field_comma_delimited("Binary")
    }

    /// The architectures this source package will build for.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-architecture>.
    pub fn architecture(&self) -> Option<Box<(dyn Iterator<Item = &str> + '_)>> {
        self.iter_field_words("Architecture")
    }

    /// The version number of the package as a string.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-version>.
    pub fn version_str(&self) -> Result<&str> {
        self.required_field_str("Version")
    }

    /// The parsed version of the source package.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-version>.
    pub fn version(&self) -> Result<PackageVersion> {
        PackageVersion::parse(self.version_str()?)
    }

    /// The package maintainer.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-maintainer>.
    pub fn maintainer(&self) -> Result<&str> {
        self.required_field_str("Maintainer")
    }

    /// The list of uploaders and co-maintainers.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-uploaders>.
    pub fn uploaders(&self) -> Option<Box<(dyn Iterator<Item = &str> + '_)>> {
        self.iter_field_comma_delimited("Uploaders")
    }

    /// The URL from which the source of this package can be obtained.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-homepage>.
    pub fn homepage(&self) -> Option<&str> {
        self.field_str("Homepage")
    }

    /// Test suites.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-testsuite>.
    pub fn testsuite(&self) -> Option<Box<(dyn Iterator<Item = &str> + '_)>> {
        self.iter_field_comma_delimited("Testsuite")
    }

    /// Describes the Git source from which this package came.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-dgit>.
    pub fn dgit(&self) -> Option<&str> {
        self.field_str("Dgit")
    }

    /// The most recent version of the standards this package conforms to.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-standards-version>.
    pub fn standards_version(&self) -> Result<&str> {
        self.required_field_str("Standards-Version")
    }

    /// The `Depends` field, parsed to a [DependencyList].
    pub fn depends(&self) -> Option<Result<DependencyList>> {
        self.field_dependency_list("Depends")
    }

    /// The `Recommends` field, parsed to a [DependencyList].
    pub fn recommends(&self) -> Option<Result<DependencyList>> {
        self.field_dependency_list("Recommends")
    }

    /// The `Suggests` field, parsed to a [DependencyList].
    pub fn suggests(&self) -> Option<Result<DependencyList>> {
        self.field_dependency_list("Suggests")
    }

    /// The `Enhances` field, parsed to a [DependencyList].
    pub fn enhances(&self) -> Option<Result<DependencyList>> {
        self.field_dependency_list("Enhances")
    }

    /// The `Pre-Depends` field, parsed to a [DependencyList].
    pub fn pre_depends(&self) -> Option<Result<DependencyList>> {
        self.field_dependency_list("Pre-Depends")
    }

    /// Obtain parsed values of all fields defining dependencies.
    pub fn package_dependency_fields(&self) -> Result<PackageDependencyFields> {
        PackageDependencyFields::from_paragraph(self)
    }

    /// Packages that can be built from this source package.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-package-list>.
    pub fn package_list(
        &self,
    ) -> Option<Box<(dyn Iterator<Item = Result<DebianSourceControlFilePackage<'_>>> + '_)>> {
        if let Some(iter) = self.iter_field_lines("Package-List") {
            Some(Box::new(iter.map(move |v| {
                let mut words = v.split_ascii_whitespace();

                let name = words
                    .next()
                    .ok_or(DebianError::ControlPackageListMissingField("name"))?;
                let package_type = words
                    .next()
                    .ok_or(DebianError::ControlPackageListMissingField("type"))?;
                let section = words
                    .next()
                    .ok_or(DebianError::ControlPackageListMissingField("section"))?;
                let priority = words
                    .next()
                    .ok_or(DebianError::ControlPackageListMissingField("priority"))?;
                let extra = words.collect::<Vec<_>>();

                Ok(DebianSourceControlFilePackage {
                    name,
                    package_type,
                    section,
                    priority,
                    extra,
                })
            })))
        } else {
            None
        }
    }

    /// List of associated files with SHA-1 checksums.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-checksums>.
    pub fn checksums_sha1(
        &self,
    ) -> Option<Box<(dyn Iterator<Item = Result<DebianSourceControlFileEntry<'_>>> + '_)>> {
        self.iter_files("Checksums-Sha1", ChecksumType::Sha1)
    }

    /// List of associated files with SHA-256 checksums.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-checksums>.
    pub fn checksums_sha256(
        &self,
    ) -> Option<Box<(dyn Iterator<Item = Result<DebianSourceControlFileEntry<'_>>> + '_)>> {
        self.iter_files("Checksums-Sha256", ChecksumType::Sha256)
    }

    /// List of associated files with MD5 checksums.
    ///
    /// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-files>.
    pub fn files(
        &self,
    ) -> Result<Box<(dyn Iterator<Item = Result<DebianSourceControlFileEntry<'_>>> + '_)>> {
        self.iter_files("Files", ChecksumType::Md5)
            .ok_or_else(|| DebianError::ControlRequiredFieldMissing("Files".to_string()))
    }

    fn iter_files(
        &self,
        field: &str,
        checksum: ChecksumType,
    ) -> Option<Box<(dyn Iterator<Item = Result<DebianSourceControlFileEntry<'_>>> + '_)>> {
        if let Some(iter) = self.iter_field_lines(field) {
            Some(Box::new(iter.map(move |v| {
                // Values are of form: <digest> <size> <path>

                let mut parts = v.split_ascii_whitespace();

                let digest = parts.next().ok_or(DebianError::ReleaseMissingDigest)?;
                let size = parts.next().ok_or(DebianError::ReleaseMissingSize)?;
                let filename = parts.next().ok_or(DebianError::ReleaseMissingPath)?;

                // Are paths with spaces allowed?
                if parts.next().is_some() {
                    return Err(DebianError::ReleasePathWithSpaces(v.to_string()));
                }

                let digest = ContentDigest::from_hex_digest(checksum, digest)?;
                let size = u64::from_str(size)?;

                Ok(DebianSourceControlFileEntry {
                    filename,
                    digest,
                    size,
                })
            })))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const ZSTD_DSC: &[u8] = include_bytes!("testdata/libzstd_1.4.8+dfsg-3.dsc");

    #[test]
    fn parse_cleartext_armored() -> Result<()> {
        let cf = DebianSourceControlFile::from_armored_reader(std::io::Cursor::new(ZSTD_DSC))?;

        cf.signatures()
            .expect("PGP signatures should have been parsed");

        assert_eq!(cf.format()?, "3.0 (quilt)");
        assert_eq!(cf.source()?, "libzstd");
        assert_eq!(
            cf.binary().unwrap().collect::<Vec<_>>(),
            vec!["libzstd-dev", "libzstd1", "zstd", "libzstd1-udeb"]
        );
        assert_eq!(cf.architecture().unwrap().collect::<Vec<_>>(), vec!["any"]);
        assert_eq!(cf.version_str()?, "1.4.8+dfsg-3");
        assert_eq!(
            cf.maintainer()?,
            "Debian Med Packaging Team <debian-med-packaging@lists.alioth.debian.org>"
        );
        assert_eq!(
            cf.uploaders().unwrap().collect::<Vec<_>>(),
            vec![
                "Kevin Murray <kdmfoss@gmail.com>",
                "Olivier Sallou <osallou@debian.org>",
                "Alexandre Mestiashvili <mestia@debian.org>",
            ]
        );
        assert_eq!(cf.homepage(), Some("https://github.com/facebook/zstd"));
        assert_eq!(cf.standards_version()?, "4.6.0");
        assert_eq!(
            cf.testsuite().unwrap().collect::<Vec<_>>(),
            vec!["autopkgtest"]
        );
        assert_eq!(
            cf.package_list().unwrap().collect::<Result<Vec<_>>>()?,
            vec![
                DebianSourceControlFilePackage {
                    name: "libzstd-dev",
                    package_type: "deb",
                    section: "libdevel",
                    priority: "optional",
                    extra: vec!["arch=any"]
                },
                DebianSourceControlFilePackage {
                    name: "libzstd1",
                    package_type: "deb",
                    section: "libs",
                    priority: "optional",
                    extra: vec!["arch=any"]
                },
                DebianSourceControlFilePackage {
                    name: "libzstd1-udeb",
                    package_type: "udeb",
                    section: "debian-installer",
                    priority: "optional",
                    extra: vec!["arch=any"]
                },
                DebianSourceControlFilePackage {
                    name: "zstd",
                    package_type: "deb",
                    section: "utils",
                    priority: "optional",
                    extra: vec!["arch=any"]
                }
            ]
        );
        assert_eq!(
            cf.checksums_sha1().unwrap().collect::<Result<Vec<_>>>()?,
            vec![
                DebianSourceControlFileEntry {
                    filename: "libzstd_1.4.8+dfsg.orig.tar.xz",
                    digest: ContentDigest::sha1_hex("a24e4ccf9fc356aeaaa0783316a26bd65817c354")?,
                    size: 1331996,
                },
                DebianSourceControlFileEntry {
                    filename: "libzstd_1.4.8+dfsg-3.debian.tar.xz",
                    digest: ContentDigest::sha1_hex("896a47a2934d0fcf9faa8397d05a12b932697d1f")?,
                    size: 12184,
                }
            ]
        );
        assert_eq!(
            cf.checksums_sha256().unwrap().collect::<Result<Vec<_>>>()?,
            vec![
                DebianSourceControlFileEntry {
                    filename: "libzstd_1.4.8+dfsg.orig.tar.xz",
                    digest: ContentDigest::sha256_hex(
                        "1e8ce5c4880a6d5bd8d3186e4186607dd19b64fc98a3877fc13aeefd566d67c5"
                    )?,
                    size: 1331996,
                },
                DebianSourceControlFileEntry {
                    filename: "libzstd_1.4.8+dfsg-3.debian.tar.xz",
                    digest: ContentDigest::sha256_hex(
                        "fecd87a469d5a07b6deeeef53ed24b2f1a74ee097ce11528fe3b58540f05c147"
                    )?,
                    size: 12184,
                }
            ]
        );
        assert_eq!(
            cf.files().unwrap().collect::<Result<Vec<_>>>()?,
            vec![
                DebianSourceControlFileEntry {
                    filename: "libzstd_1.4.8+dfsg.orig.tar.xz",
                    digest: ContentDigest::md5_hex("943bed8b8d98a50c8d8a101b12693bb4")?,
                    size: 1331996,
                },
                DebianSourceControlFileEntry {
                    filename: "libzstd_1.4.8+dfsg-3.debian.tar.xz",
                    digest: ContentDigest::md5_hex("4d2692830e1f481ce769e2dd24cbc9db")?,
                    size: 12184,
                }
            ]
        );

        Ok(())
    }
}
