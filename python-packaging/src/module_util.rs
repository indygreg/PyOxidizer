// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Utility functions related to Python modules. */

use std::collections::BTreeSet;

/// Resolve the set of packages present in a fully qualified module name.
pub fn packages_from_module_name(module: &str) -> BTreeSet<String> {
    let mut package_names = BTreeSet::new();

    let mut search: &str = &module;

    while let Some(idx) = search.rfind('.') {
        package_names.insert(search[0..idx].to_string());
        search = &search[0..idx];
    }

    package_names
}

/// Resolve the set of packages present in a series of fully qualified module names.
pub fn packages_from_module_names<I>(names: I) -> BTreeSet<String>
where
    I: Iterator<Item = String>,
{
    let mut package_names = BTreeSet::new();

    for name in names {
        let mut search: &str = &name;

        while let Some(idx) = search.rfind('.') {
            package_names.insert(search[0..idx].to_string());
            search = &search[0..idx];
        }
    }

    package_names
}

#[cfg(test)]
mod tests {
    use {super::*, std::iter::FromIterator};

    #[test]
    fn test_packages_from_module_name() {
        assert_eq!(
            packages_from_module_name("foo.bar"),
            BTreeSet::from_iter(vec!["foo".to_string()])
        );
        assert_eq!(
            packages_from_module_name("foo.bar.baz"),
            BTreeSet::from_iter(vec!["foo".to_string(), "foo.bar".to_string()])
        );
    }
}
