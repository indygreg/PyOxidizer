// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

extern crate pyrepackager;

use std::env;
use std::path::Path;

use pyrepackager::repackage::process_config;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PYOXIDIZER_CONFIG");

    let config_env =
        env::var("PYOXIDIZER_CONFIG").expect("PYOXIDIZER_CONFIG environment variable not set");
    let config_path = Path::new(&config_env);

    if !config_path.exists() {
        panic!(
            "config file {} defined by PYOXIDIZER_CONFIG does not exist",
            config_env
        );
    }

    println!("cargo:rerun-if-changed={}", config_env);

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_dir_path = Path::new(&out_dir);

    process_config(config_path, out_dir_path);
}
