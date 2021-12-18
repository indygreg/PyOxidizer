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
