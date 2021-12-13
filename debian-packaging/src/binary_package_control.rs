// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian binary package control files. */

use crate::{
    control::ControlParagraph,
    dependency::{DependencyList, PackageDependencyFields},
    error::{DebianError, Result},
    io::ContentDigest,
    package_version::PackageVersion,
    repository::{builder::DebPackageReference, release::ChecksumType},
};

/// A Debian binary package control file.
///
/// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#binary-package-control-files-debian-control>.
///
/// Binary package control files are defined by a single paragraph with well-defined
/// fields. This type is a low-level wrapper around an inner [ControlParagraph].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BinaryPackageControlFile<'a> {
    paragraph: ControlParagraph<'a>,
}

impl<'a> AsRef<ControlParagraph<'a>> for BinaryPackageControlFile<'a> {
    fn as_ref(&self) -> &ControlParagraph<'a> {
        &self.paragraph
    }
}

impl<'a> AsMut<ControlParagraph<'a>> for BinaryPackageControlFile<'a> {
    fn as_mut(&mut self) -> &mut ControlParagraph<'a> {
        &mut self.paragraph
    }
}

impl<'a> From<ControlParagraph<'a>> for BinaryPackageControlFile<'a> {
    fn from(paragraph: ControlParagraph<'a>) -> Self {
        Self { paragraph }
    }
}

impl<'a> BinaryPackageControlFile<'a> {
    pub fn package(&self) -> Result<&str> {
        self.paragraph.required_field_str("Package")
    }

    /// The `Version` field as its original string.
    pub fn version_str(&self) -> Result<&str> {
        self.paragraph.required_field_str("Version")
    }

    /// The `Version` field parsed into a [PackageVersion].
    pub fn version(&self) -> Result<PackageVersion> {
        PackageVersion::parse(self.version_str()?)
    }

    pub fn architecture(&self) -> Result<&str> {
        self.paragraph.required_field_str("Architecture")
    }

    pub fn maintainer(&self) -> Result<&str> {
        self.paragraph.required_field_str("Maintainer")
    }

    pub fn description(&self) -> Result<&str> {
        self.paragraph.required_field_str("Description")
    }

    pub fn source(&self) -> Option<&str> {
        self.paragraph.field_str("Source")
    }

    pub fn section(&self) -> Option<&str> {
        self.paragraph.field_str("Section")
    }

    pub fn priority(&self) -> Option<&str> {
        self.paragraph.field_str("Priority")
    }

    pub fn essential(&self) -> Option<&str> {
        self.paragraph.field_str("Essential")
    }

    pub fn homepage(&self) -> Option<&str> {
        self.paragraph.field_str("Homepage")
    }

    pub fn installed_size(&self) -> Option<Result<usize>> {
        self.paragraph.field_usize("Installed-Size")
    }

    pub fn size(&self) -> Option<Result<usize>> {
        self.paragraph.field_usize("Size")
    }

    pub fn built_using(&self) -> Option<&str> {
        self.paragraph.field_str("Built-Using")
    }

    pub fn depends(&self) -> Option<Result<DependencyList>> {
        self.paragraph.field_dependency_list("Depends")
    }

    pub fn recommends(&self) -> Option<Result<DependencyList>> {
        self.paragraph.field_dependency_list("Recommends")
    }

    pub fn suggests(&self) -> Option<Result<DependencyList>> {
        self.paragraph.field_dependency_list("Suggests")
    }

    pub fn enhances(&self) -> Option<Result<DependencyList>> {
        self.paragraph.field_dependency_list("Enhances")
    }

    pub fn pre_depends(&self) -> Option<Result<DependencyList>> {
        self.paragraph.field_dependency_list("Pre-Depends")
    }

    /// Obtain parsed values of all fields defining dependencies.
    pub fn package_dependency_fields(&self) -> Result<PackageDependencyFields> {
        PackageDependencyFields::from_paragraph(&self.paragraph)
    }
}

impl<'cf, 'a: 'cf> DebPackageReference<'cf> for BinaryPackageControlFile<'a> {
    fn deb_size_bytes(&self) -> Result<usize> {
        self.size()
            .ok_or_else(|| DebianError::ControlRequiredFieldMissing("Size".to_string()))?
    }

    fn deb_digest(&self, checksum: ChecksumType) -> Result<ContentDigest> {
        let hex_digest = self
            .paragraph
            .field_str(checksum.field_name())
            .ok_or_else(|| {
                DebianError::ControlRequiredFieldMissing(checksum.field_name().to_string())
            })?;

        let digest = hex::decode(hex_digest)?;

        Ok(match checksum {
            ChecksumType::Md5 => ContentDigest::Md5(digest),
            ChecksumType::Sha1 => ContentDigest::Sha1(digest),
            ChecksumType::Sha256 => ContentDigest::Sha256(digest),
        })
    }

    fn deb_filename(&self) -> Result<String> {
        let filename = self
            .paragraph
            .field_str("Filename")
            .ok_or_else(|| DebianError::ControlRequiredFieldMissing("Filename".to_string()))?;

        Ok(if let Some((_, s)) = filename.rsplit_once('/') {
            s.to_string()
        } else {
            filename.to_string()
        })
    }

    fn control_file_for_packages_index(&self) -> Result<BinaryPackageControlFile<'cf>> {
        Ok(self.clone())
    }
}
