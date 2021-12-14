// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian binary package control files. */

use {
    crate::{
        control::ControlParagraph,
        dependency::{DependencyList, PackageDependencyFields},
        error::{DebianError, Result},
        io::ContentDigest,
        package_version::PackageVersion,
        repository::{builder::DebPackageReference, release::ChecksumType},
    },
    std::ops::{Deref, DerefMut},
};

/// A Debian binary package control file/paragraph.
///
/// See <https://www.debian.org/doc/debian-policy/ch-controlfields.html#binary-package-control-files-debian-control>.
///
/// Binary package control files are defined by a single paragraph with well-defined
/// fields. This type is a low-level wrapper around an inner [ControlParagraph].
/// [Deref] and [DerefMut] can be used to operate on the inner [ControlParagraph].
/// [From] and [Into] are implemented in both directions to enable cheap coercion
/// between the types.
///
/// Binary package control paragraphs are seen in `DEBIAN/control` files. Variations
/// also exist in `Packages` files in repositories and elsewhere.
///
/// Fields annotated as *mandatory* in the Debian Policy Manual have getters that
/// return [Result] and will error if a field is not present. Non-mandatory fields
/// return [Option]. This enforcement can be bypassed by calling
/// [ControlParagraph::field()].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BinaryPackageControlFile<'a> {
    paragraph: ControlParagraph<'a>,
}

impl<'a> Deref for BinaryPackageControlFile<'a> {
    type Target = ControlParagraph<'a>;

    fn deref(&self) -> &Self::Target {
        &self.paragraph
    }
}

impl<'a> DerefMut for BinaryPackageControlFile<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.paragraph
    }
}

impl<'a> From<ControlParagraph<'a>> for BinaryPackageControlFile<'a> {
    fn from(paragraph: ControlParagraph<'a>) -> Self {
        Self { paragraph }
    }
}

impl<'a> From<BinaryPackageControlFile<'a>> for ControlParagraph<'a> {
    fn from(cf: BinaryPackageControlFile<'a>) -> Self {
        cf.paragraph
    }
}

impl<'a> BinaryPackageControlFile<'a> {
    /// The `Package` field value.
    pub fn package(&self) -> Result<&str> {
        self.required_field_str("Package")
    }

    /// The `Version` field as its original string.
    pub fn version_str(&self) -> Result<&str> {
        self.required_field_str("Version")
    }

    /// The `Version` field parsed into a [PackageVersion].
    pub fn version(&self) -> Result<PackageVersion> {
        PackageVersion::parse(self.version_str()?)
    }

    /// The `Architecture` field.
    pub fn architecture(&self) -> Result<&str> {
        self.required_field_str("Architecture")
    }

    /// The `Maintainer` field.
    pub fn maintainer(&self) -> Result<&str> {
        self.required_field_str("Maintainer")
    }

    /// The `Description` field.
    pub fn description(&self) -> Result<&str> {
        self.required_field_str("Description")
    }

    /// The `Source` field.
    pub fn source(&self) -> Option<&str> {
        self.field_str("Source")
    }

    /// The `Section` field.
    pub fn section(&self) -> Option<&str> {
        self.field_str("Section")
    }

    /// The `Priority` field.
    pub fn priority(&self) -> Option<&str> {
        self.field_str("Priority")
    }

    /// The `Essential` field.
    pub fn essential(&self) -> Option<&str> {
        self.field_str("Essential")
    }

    /// The `Homepage` field.
    pub fn homepage(&self) -> Option<&str> {
        self.field_str("Homepage")
    }

    /// The `Installed-Size` field, parsed to a [u64].
    pub fn installed_size(&self) -> Option<Result<u64>> {
        self.field_u64("Installed-Size")
    }

    /// The `Size` field, parsed to a [u64].
    pub fn size(&self) -> Option<Result<u64>> {
        self.field_u64("Size")
    }

    /// The `Built-Using` field.
    pub fn built_using(&self) -> Option<&str> {
        self.field_str("Built-Using")
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
        PackageDependencyFields::from_paragraph(&self)
    }
}

impl<'cf, 'a: 'cf> DebPackageReference<'cf> for BinaryPackageControlFile<'a> {
    fn deb_size_bytes(&self) -> Result<u64> {
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

        ContentDigest::from_hex_digest(checksum, hex_digest)
    }

    fn deb_filename(&self) -> Result<String> {
        let filename = self
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
