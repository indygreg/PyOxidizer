// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Interface with a collection of binary package control definitions. */

use {
    crate::{
        binary_package_control::{BinaryPackageControlError, BinaryPackageControlFile},
        control::ControlError,
    },
    std::ops::{Deref, DerefMut},
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum BinaryPackageListError {
    #[error("control file error: {0:?}")]
    Control(#[from] ControlError),

    #[error("binary package control error: {0:?}")]
    BinaryPackageControl(#[from] BinaryPackageControlError),
}

pub type Result<T> = std::result::Result<T, BinaryPackageListError>;

/// Represents a collection of binary package control files.
///
/// Various operations in Debian packaging operate against a collection of
/// binary package control files. For example, resolving dependencies of a
/// package requires finding packages from an available set. This type facilitates
/// the implementation of said operations.
#[derive(Clone, Debug, Default)]
pub struct BinaryPackageList<'a> {
    packages: Vec<BinaryPackageControlFile<'a>>,
}

impl<'a> Deref for BinaryPackageList<'a> {
    type Target = Vec<BinaryPackageControlFile<'a>>;

    fn deref(&self) -> &Self::Target {
        &self.packages
    }
}

impl<'a> DerefMut for BinaryPackageList<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.packages
    }
}

impl<'a> IntoIterator for BinaryPackageList<'a> {
    type Item = BinaryPackageControlFile<'a>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.packages.into_iter()
    }
}

impl<'a> BinaryPackageList<'a> {
    /// Find instances of a package within this collection.
    pub fn find_packages_with_name(
        &self,
        package: String,
    ) -> impl Iterator<Item = &BinaryPackageControlFile<'a>> {
        self.packages
            .iter()
            .filter(move |cf| matches!(cf.package(), Ok(name) if name == package))
    }
}

#[cfg(test)]
mod test {

    use {super::*, crate::control::ControlParagraphReader, indoc::indoc, std::io::Cursor};

    const FOO_1_2: &str = indoc! {"
        Package: foo
        Version: 1.2
        Installed-Size: 20268
        Architecture: amd64
    "};

    const BAR_1_0: &str = indoc! {"
        Package: bar
        Version: 1.0
        Architecture: amd64
        Depends: foo (>= 1.2)
    "};

    const BAZ_1_1: &str = indoc! {"
        Package: baz
        Version: 1.1
        Architecture: amd64
        Depends: bar (>= 1.0)
    "};

    #[test]
    fn find_package() -> Result<()> {
        let foo_para = ControlParagraphReader::new(Cursor::new(FOO_1_2.as_bytes()))
            .next()
            .unwrap()?;

        let bar_para = ControlParagraphReader::new(Cursor::new(BAR_1_0.as_bytes()))
            .next()
            .unwrap()?;

        let baz_para = ControlParagraphReader::new(Cursor::new(BAZ_1_1.as_bytes()))
            .next()
            .unwrap()?;

        let mut l = BinaryPackageList::default();
        l.push(BinaryPackageControlFile::from(foo_para));
        l.push(BinaryPackageControlFile::from(bar_para));
        l.push(BinaryPackageControlFile::from(baz_para));

        assert_eq!(l.find_packages_with_name("other".into()).count(), 0);

        let packages = l.find_packages_with_name("foo".into()).collect::<Vec<_>>();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].version_str()?, "1.2");

        let packages = l.find_packages_with_name("bar".into()).collect::<Vec<_>>();
        assert_eq!(packages.len(), 1);

        Ok(())
    }
}
