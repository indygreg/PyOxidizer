// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian binary package control files. */

use {
    crate::{
        control::{ControlField, ControlParagraph},
        dependency::{DependencyError, DependencyList, PackageDependencyFields},
        io::ContentDigest,
        package_version::{PackageVersion, VersionError},
        repository::{
            builder::{DebPackageReference, Result as RepositoryBuilderResult},
            release::ChecksumType,
        },
    },
    std::{num::ParseIntError, str::FromStr},
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum BinaryPackageControlError {
    #[error("required field missing: {0}")]
    RequiredFieldMissing(&'static str),

    #[error("integer parsing error: {0}")]
    IntegerParse(#[from] ParseIntError),

    #[error("dependency error: {0:?}")]
    Depends(#[from] DependencyError),

    #[error("version error: {0:?}")]
    Version(#[from] VersionError),

    #[error("invalid hexadecimal in content digest: {0:?}")]
    FromHex(#[from] hex::FromHexError),
}

pub type Result<T> = std::result::Result<T, BinaryPackageControlError>;

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
    /// Obtain the first occurrence of the given field.
    pub fn first_field(&self, name: &str) -> Option<&ControlField<'_>> {
        self.paragraph.first_field(name)
    }

    /// Obtain the string value of the first occurrence of the given field.
    pub fn first_field_str(&self, name: &str) -> Option<&str> {
        self.paragraph.first_field_str(name)
    }

    /// Obtain the first value of a field, evaluated as a boolean.
    ///
    /// The field is [true] iff its string value is `yes`.
    pub fn first_field_bool(&self, name: &str) -> Option<bool> {
        self.paragraph
            .first_field_str(name)
            .map(|v| matches!(v, "yes"))
    }

    fn required_field(&self, field: &'static str) -> Result<&str> {
        self.paragraph
            .first_field_str(field)
            .ok_or(BinaryPackageControlError::RequiredFieldMissing(field))
    }

    pub fn package(&self) -> Result<&str> {
        self.required_field("Package")
    }

    /// The `Version` field as its original string.
    pub fn version_str(&self) -> Result<&str> {
        self.required_field("Version")
    }

    /// The `Version` field parsed into a [PackageVersion].
    pub fn version(&self) -> Result<PackageVersion> {
        Ok(PackageVersion::parse(self.version_str()?)?)
    }

    pub fn architecture(&self) -> Result<&str> {
        self.required_field("Architecture")
    }

    pub fn maintainer(&self) -> Result<&str> {
        self.required_field("Maintainer")
    }

    pub fn description(&self) -> Result<&str> {
        self.required_field("Description")
    }

    pub fn source(&self) -> Option<&str> {
        self.paragraph.first_field_str("Source")
    }

    pub fn section(&self) -> Option<&str> {
        self.paragraph.first_field_str("Section")
    }

    pub fn priority(&self) -> Option<&str> {
        self.paragraph.first_field_str("Priority")
    }

    pub fn essential(&self) -> Option<&str> {
        self.paragraph.first_field_str("Essential")
    }

    pub fn homepage(&self) -> Option<&str> {
        self.paragraph.first_field_str("Homepage")
    }

    pub fn installed_size(&self) -> Option<Result<usize>> {
        self.paragraph
            .first_field_str("Installed-Size")
            .map(|x| Ok(usize::from_str(x)?))
    }

    pub fn size(&self) -> Option<Result<usize>> {
        self.paragraph
            .first_field_str("Size")
            .map(|x| Ok(usize::from_str(x)?))
    }

    pub fn built_using(&self) -> Option<&str> {
        self.paragraph.first_field_str("Built-Using")
    }

    pub fn depends(&self) -> Option<Result<DependencyList>> {
        self.paragraph
            .first_field_str("Depends")
            .map(|x| Ok(DependencyList::parse(x)?))
    }

    pub fn recommends(&self) -> Option<Result<DependencyList>> {
        self.paragraph
            .first_field_str("Recommends")
            .map(|x| Ok(DependencyList::parse(x)?))
    }

    pub fn suggests(&self) -> Option<Result<DependencyList>> {
        self.paragraph
            .first_field_str("Suggests")
            .map(|x| Ok(DependencyList::parse(x)?))
    }

    pub fn enhances(&self) -> Option<Result<DependencyList>> {
        self.paragraph
            .first_field_str("Enhances")
            .map(|x| Ok(DependencyList::parse(x)?))
    }

    pub fn pre_depends(&self) -> Option<Result<DependencyList>> {
        self.paragraph
            .first_field_str("Pre-Depends")
            .map(|x| Ok(DependencyList::parse(x)?))
    }

    /// Obtain parsed values of all fields defining dependencies.
    pub fn package_dependency_fields(&self) -> Result<PackageDependencyFields> {
        Ok(PackageDependencyFields::from_paragraph(&self.paragraph)?)
    }
}

impl<'cf, 'a: 'cf> DebPackageReference<'cf> for BinaryPackageControlFile<'a> {
    fn size_bytes(&self) -> RepositoryBuilderResult<usize> {
        Ok(self
            .size()
            .ok_or_else(|| BinaryPackageControlError::RequiredFieldMissing("Size"))??)
    }

    fn digest(&self, checksum: ChecksumType) -> RepositoryBuilderResult<ContentDigest> {
        let hex_digest = self.first_field_str(checksum.field_name()).ok_or_else(|| {
            BinaryPackageControlError::RequiredFieldMissing(checksum.field_name())
        })?;

        let digest = hex::decode(hex_digest).map_err(BinaryPackageControlError::FromHex)?;

        Ok(match checksum {
            ChecksumType::Md5 => ContentDigest::Md5(digest),
            ChecksumType::Sha1 => ContentDigest::Sha1(digest),
            ChecksumType::Sha256 => ContentDigest::Sha256(digest),
        })
    }

    fn filename(&self) -> RepositoryBuilderResult<String> {
        let filename = self
            .first_field_str("Filename")
            .ok_or(BinaryPackageControlError::RequiredFieldMissing("Filename"))?;

        Ok(if let Some((_, s)) = filename.rsplit_once('/') {
            s.to_string()
        } else {
            filename.to_string()
        })
    }

    fn control_file_for_packages_index(
        &self,
    ) -> RepositoryBuilderResult<BinaryPackageControlFile<'cf>> {
        Ok(self.clone())
    }
}
