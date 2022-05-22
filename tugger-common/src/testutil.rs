// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {once_cell::sync::Lazy, std::path::PathBuf};

pub static DEFAULT_TEMP_DIR: Lazy<tempfile::TempDir> = Lazy::new(|| {
    tempfile::Builder::new()
        .prefix("tugger-test")
        .tempdir()
        .expect("unable to create temporary directory")
});

pub static DEFAULT_DOWNLOAD_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let p = if let Ok(manifest_dir) = std::env::var("OUT_DIR") {
        PathBuf::from(manifest_dir).join("tugger-files")
    } else {
        DEFAULT_TEMP_DIR.path().join("tugger-files")
    };

    std::fs::create_dir_all(&p).expect("unable to create download directory");

    p
});
