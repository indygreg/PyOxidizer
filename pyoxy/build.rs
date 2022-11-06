// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=PYO3_CONFIG_FILE");

    // By default Rust will not export dynamic symbols from built executables.
    // If we're linking libpython, we need its symbols to be exported in order to
    // load Python extension modules.
    if let Ok(os) = std::env::var("CARGO_CFG_TARGET_OS") {
        match os.as_str() {
            "linux" => {
                println!("cargo:rustc-link-arg=-Wl,-export-dynamic");
            }
            "macos" => {
                println!("cargo:rustc-link-arg=-rdynamic");
            }
            _ => {}
        }
    }

    // If a PyO3 config file is defined, we look for PyOxidizer's packed resources
    // in the same directory. If found, we make those resources available to the crate.
    if let Ok(config_path) = std::env::var("PYO3_CONFIG_FILE") {
        let config_path = PathBuf::from(config_path);
        println!("cargo:rerun-if-changed={}", config_path.display());

        let artifact_dir = config_path
            .parent()
            .expect("could not resolve parent directory of PyO3 config file");

        let packed_resources_path = artifact_dir.join("packed-resources");

        if packed_resources_path.exists() {
            println!("cargo:rerun-if-changed={}", packed_resources_path.display());
            println!("cargo:rustc-cfg=stdlib_packed_resources");
            println!(
                "cargo:rustc-env=PYTHON_PACKED_RESOURCES_PATH={}",
                packed_resources_path.display()
            );
        }
    }
}
