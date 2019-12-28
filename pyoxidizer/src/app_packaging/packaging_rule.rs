// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::BTreeSet;

use crate::py_packaging::resource::AppRelativeResources;

impl AppRelativeResources {
    pub fn package_names(&self) -> BTreeSet<String> {
        let mut packages = packages_from_module_names(self.module_sources.keys().cloned());
        packages.extend(packages_from_module_names(
            self.module_bytecodes.keys().cloned(),
        ));

        packages
    }
}

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
