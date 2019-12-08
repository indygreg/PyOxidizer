// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
    pub dont_write_bytecode: bool,
    pub ignore_environment: bool,
    pub inspect: bool,
    pub interactive: bool,
    pub isolated: bool,
    pub legacy_windows_fs_encoding: bool,
    pub legacy_windows_stdio: bool,
    pub no_site: bool,
    pub no_user_site_directory: bool,
    pub optimize_level: i64,
    pub parser_debug: bool,
    pub quiet: bool,
    pub stdio_encoding_name: Option<String>,
    pub stdio_encoding_errors: Option<String>,
    pub unbuffered_stdio: bool,
    pub use_hash_seed: bool,
    pub verbose: i32,
    pub filesystem_importer: bool,
    pub sys_frozen: bool,
    pub sys_meipass: bool,
    pub sys_paths: Vec<String>,
    pub raw_allocator: RawAllocator,
    pub terminfo_resolution: TerminfoResolution,
    pub write_modules_directory_env: Option<String>,
}

impl Default for EmbeddedPythonConfig {
    fn default() -> Self {
        EmbeddedPythonConfig {
            bytes_warning: 0,
            dont_write_bytecode: true,
            ignore_environment: true,
            inspect: false,
            interactive: false,
            isolated: false,
            legacy_windows_fs_encoding: false,
            legacy_windows_stdio: false,
            no_site: true,
            no_user_site_directory: true,
            optimize_level: 0,
            parser_debug: false,
            quiet: false,
            stdio_encoding_name: None,
            stdio_encoding_errors: None,
            unbuffered_stdio: false,
            use_hash_seed: false,
            verbose: 0,
            filesystem_importer: false,
            sys_frozen: false,
            sys_meipass: false,
            sys_paths: Vec::new(),
            raw_allocator: RawAllocator::System,
            terminfo_resolution: TerminfoResolution::None,
            write_modules_directory_env: None,
        }
    }
}
