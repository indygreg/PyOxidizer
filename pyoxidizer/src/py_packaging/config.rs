// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Configuring a Python interpreter.
*/

/// Determine the default raw allocator for a target triple.
pub fn default_raw_allocator(target_triple: &str) -> RawAllocator {
    // Jemalloc doesn't work on Windows.
    //
    // We don't use Jemalloc by default in the test environment because it slows down
    // builds of test projects.
    if target_triple == "x86_64-pc-windows-msvc" || cfg!(test) {
        RawAllocator::System
    } else {
        RawAllocator::Jemalloc
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RawAllocator {
    Jemalloc,
    Rust,
    System,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RunMode {
    Noop,
    Repl,
    Module { module: String },
    Eval { code: String },
    File { path: String },
}

/// How the `terminfo` database is resolved at run-time.
#[derive(Clone, Debug, PartialEq)]
pub enum TerminfoResolution {
    Dynamic,
    None,
    Static(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddedPythonConfig {
    pub bytes_warning: i32,
    pub ignore_environment: bool,
    pub inspect: bool,
    pub interactive: bool,
    pub isolated: bool,
    pub legacy_windows_fs_encoding: bool,
    pub legacy_windows_stdio: bool,
    pub optimize_level: i64,
    pub parser_debug: bool,
    pub stdio_encoding_name: Option<String>,
    pub stdio_encoding_errors: Option<String>,
    pub unbuffered_stdio: bool,
    pub filesystem_importer: bool,
    pub quiet: bool,
    pub raw_allocator: RawAllocator,
    pub run_mode: RunMode,
    pub site_import: bool,
    pub sys_frozen: bool,
    pub sys_meipass: bool,
    pub sys_paths: Vec<String>,
    pub terminfo_resolution: TerminfoResolution,
    pub use_hash_seed: bool,
    pub user_site_directory: bool,
    pub verbose: i32,
    pub write_bytecode: bool,
    pub write_modules_directory_env: Option<String>,
}

impl Default for EmbeddedPythonConfig {
    fn default() -> Self {
        EmbeddedPythonConfig {
            bytes_warning: 0,
            ignore_environment: true,
            inspect: false,
            interactive: false,
            isolated: true,
            legacy_windows_fs_encoding: false,
            legacy_windows_stdio: false,
            optimize_level: 0,
            parser_debug: false,
            quiet: false,
            stdio_encoding_name: None,
            stdio_encoding_errors: None,
            unbuffered_stdio: false,
            use_hash_seed: false,
            verbose: 0,
            filesystem_importer: false,
            site_import: false,
            sys_frozen: false,
            sys_meipass: false,
            sys_paths: Vec::new(),
            raw_allocator: RawAllocator::System,
            run_mode: RunMode::Repl,
            terminfo_resolution: TerminfoResolution::None,
            user_site_directory: false,
            write_bytecode: false,
            write_modules_directory_env: None,
        }
    }
}
