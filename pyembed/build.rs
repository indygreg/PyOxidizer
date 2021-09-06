// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

fn main() {
    let library_mode = if std::env::var("CARGO_FEATURE_EXTENSION_MODULE").is_ok() {
        "extension"
    } else {
        "pyembed"
    };

    // We're always able to derive this. So always set it, even though it is likely
    // only used by test mode.
    println!(
        "cargo:rustc-env=PYEMBED_TESTS_DIR={}/src/test",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    );

    println!("cargo:rustc-cfg=library_mode=\"{}\"", library_mode);

    let interpreter_config = pyo3_build_config::get();

    // Re-export the path to the configured Python interpreter. Tests can
    // use this to derive a useful default config that leverages it.
    let python_interpreter = interpreter_config
        .executable
        .as_ref()
        .expect("PyO3 configuration does not define Python executable path");

    println!(
        "cargo:rustc-env=PYTHON_INTERPRETER_PATH={}",
        python_interpreter
    );
}
