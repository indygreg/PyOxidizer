// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;
use slog::warn;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

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

pub fn filter_btreemap<V>(
    logger: &slog::Logger,
    m: &mut BTreeMap<String, V>,
    f: &BTreeSet<String>,
) {
    let keys: Vec<String> = m.keys().cloned().collect();

    for key in keys {
        if !f.contains(&key) {
            warn!(logger, "removing {}", key);
            m.remove(&key);
        }
    }
}
