// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Utility code for filtering.
*/

use {
    anyhow::{anyhow, Result},
    log::warn,
    std::{
        collections::{BTreeMap, BTreeSet},
        fs::File,
        io::{BufRead, BufReader},
        path::Path,
    },
};

pub fn read_resource_names_file(path: &Path) -> Result<BTreeSet<String>> {
    let fh = File::open(path)?;

    let mut res: BTreeSet<String> = BTreeSet::new();

    for line in BufReader::new(fh).lines() {
        let line = line?;

        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        res.insert(line);
    }

    Ok(res)
}

pub fn resolve_resource_names_from_files(
    files: &[&Path],
    glob_files: &[&str],
) -> Result<BTreeSet<String>> {
    let mut include_names = BTreeSet::new();

    for path in files {
        let new_names = read_resource_names_file(path)?;
        include_names.extend(new_names);
    }

    for pattern in glob_files {
        let mut new_names = BTreeSet::new();

        for entry in glob::glob(pattern)? {
            new_names.extend(read_resource_names_file(&entry?)?);
        }

        if new_names.is_empty() {
            return Err(anyhow!(
                "glob filter resolves to empty set; are you sure the glob pattern is correct?"
            ));
        }

        include_names.extend(new_names);
    }

    Ok(include_names)
}

pub fn filter_btreemap<V>(m: &mut BTreeMap<String, V>, f: &BTreeSet<String>) {
    let keys: Vec<String> = m.keys().cloned().collect();

    for key in keys {
        if !f.contains(&key) {
            warn!("removing {}", key);
            m.remove(&key);
        }
    }
}
