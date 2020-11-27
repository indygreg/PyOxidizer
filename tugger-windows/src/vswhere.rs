// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    std::path::PathBuf,
};

#[cfg(windows)]
use crate::util::get_known_folder_path;

/// Attempt to locate vswhere.exe.
#[cfg(windows)]
pub fn find_vswhere() -> Result<PathBuf> {
    let candidates = vec![
        (
            winapi::um::knownfolders::FOLDERID_ProgramFilesX86,
            r"Microsoft Visual Studio\Installer\vswhere.exe",
        ),
        (
            winapi::um::knownfolders::FOLDERID_ProgramData,
            r"Microsoft Visual Studio\Installer\vswhere.exe",
        ),
    ];

    for (well_known, path) in candidates {
        let path = get_known_folder_path(&well_known).map(|p| p.join(path))?;

        if path.exists() {
            return Ok(path);
        }
    }

    Err(anyhow!("could not find vswhere.exe"))
}

/// Attempt to locate vswhere.exe.
#[cfg(unix)]
pub fn find_vswhere() -> Result<PathBuf> {
    Err(anyhow!("finding vswhere.exe only supported on Windows"))
}
