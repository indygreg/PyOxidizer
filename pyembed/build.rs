// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

fn main() {
    // We're always able to derive this. So always set it, even though it is likely
    // only used by test mode.
    println!(
        "cargo:rustc-env=PYEMBED_TESTS_DIR={}/src/test",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    );

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
