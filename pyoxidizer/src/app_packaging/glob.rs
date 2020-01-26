// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::Result,
    std::path::{Path, PathBuf},
};

/// Evaluate a file matching glob relative to the current working directory.
pub fn evaluate_glob(cwd: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    let pattern_path = PathBuf::from(pattern);

    let search = if pattern.starts_with('/') || pattern_path.is_absolute() {
        pattern.to_string()
    } else {
        format!("{}/{}", cwd.display(), pattern)
    };

    let mut res = Vec::new();

    for path in glob::glob(&search)? {
        let path = path?;

        if path.is_file() {
            res.push(path);
        }
    }

    Ok(res)
}
