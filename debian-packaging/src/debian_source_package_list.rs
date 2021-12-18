// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! A collection of source control package control files. */

use {
    crate::debian_source_control::DebianSourceControlFile,
    std::ops::{Deref, DerefMut},
};

/// Represents a collection of Debian source control paragraphs.
///
/// This provides a wrapper around [Vec<DebianSourceControlFile>] for convenience.
///
/// Note that [DebianSourceControlFile] within this collection may not conform to the
/// strict requirements of Debian source control `.dsc` files. For example, the
/// `Source` field may not be present (try `Package` instead).
#[derive(Default)]
pub struct DebianSourcePackageList<'a> {
    packages: Vec<DebianSourceControlFile<'a>>,
}

impl<'a> Deref for DebianSourcePackageList<'a> {
    type Target = Vec<DebianSourceControlFile<'a>>;

    fn deref(&self) -> &Self::Target {
        &self.packages
    }
}

impl<'a> DerefMut for DebianSourcePackageList<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.packages
    }
}

impl<'a> IntoIterator for DebianSourcePackageList<'a> {
    type Item = DebianSourceControlFile<'a>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.packages.into_iter()
    }
}

impl<'a> DebianSourcePackageList<'a> {
    /// Find source packages having the given name.
    ///
    /// This patches against the `Package` field in the control files.
    pub fn iter_with_package_name(
        &self,
        package: String,
    ) -> impl Iterator<Item = &DebianSourceControlFile<'a>> {
        self.packages.iter().filter(
            move |cf| matches!(cf.required_field_str("Package"), Ok(name) if name == package),
        )
    }

    /// Find source packages providing the given binary package.
    ///
    /// This consults the list of binary packages in the `Binary` field and returns control
    /// paragraphs where `package` appears in that list.
    pub fn iter_with_binary_package(
        &self,
        package: String,
    ) -> impl Iterator<Item = &DebianSourceControlFile<'a>> {
        self.packages.iter().filter(move |cf| {
            if let Some(mut packages) = cf.binary() {
                packages.any(|p| p == package)
            } else {
                false
            }
        })
    }

    /// Find source packages providing packages for the given architecture.
    ///
    /// This consults the list of architectures in the `Architecture` field and returns
    /// control paragraphs where `architecture` appears in that list.
    pub fn iter_with_architecture(
        &self,
        architecture: String,
    ) -> impl Iterator<Item = &DebianSourceControlFile<'a>> {
        self.packages.iter().filter(move |cf| {
            if let Some(mut architectures) = cf.architecture() {
                architectures.any(|a| a == architecture)
            } else {
                false
            }
        })
    }
}
