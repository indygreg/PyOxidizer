// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Functionality related to the pyembed crate.
*/

use anyhow::Result;
use itertools::Itertools;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::config::{EmbeddedPythonConfig, RawAllocator, RunMode, TerminfoResolution};

/// Obtain the Rust source code to construct a PythonConfig instance.
pub fn derive_python_config(
    embedded: &EmbeddedPythonConfig,
    embedded_resources_path: &PathBuf,
) -> String {
    format!(
        "pyembed::PythonConfig {{\n    \
         standard_io_encoding: {},\n    \
         standard_io_errors: {},\n    \
         opt_level: {},\n    \
         use_custom_importlib: true,\n    \
         filesystem_importer: {},\n    \
         sys_paths: [{}].to_vec(),\n    \
         bytes_warning: {},\n    \
         import_site: {},\n    \
         import_user_site: {},\n    \
         ignore_python_env: {},\n    \
         inspect: {},\n    \
         interactive: {},\n    \
         isolated: {},\n    \
         legacy_windows_fs_encoding: {},\n    \
         legacy_windows_stdio: {},\n    \
         write_bytecode: {},\n    \
         unbuffered_stdio: {},\n    \
         parser_debug: {},\n    \
         quiet: {},\n    \
         use_hash_seed: {},\n    \
         verbose: {},\n    \
         packed_resources: include_bytes!(r#\"{}\"#),\n    \
         extra_extension_modules: vec![],\n    \
         argvb: false,\n    \
         sys_frozen: {},\n    \
         sys_meipass: {},\n    \
         raw_allocator: {},\n    \
         terminfo_resolution: {},\n    \
         write_modules_directory_env: {},\n    \
         run: {},\n\
         }}",
        match &embedded.stdio_encoding_name {
            Some(value) => format_args!("Some(\"{}\")", value).to_string(),
            None => "None".to_owned(),
        },
        match &embedded.stdio_encoding_errors {
            Some(value) => format_args!("Some(\"{}\")", value).to_string(),
            None => "None".to_owned(),
        },
        embedded.optimize_level,
        embedded.filesystem_importer,
        &embedded
            .sys_paths
            .iter()
            .map(|p| "\"".to_owned() + p + "\".to_string()")
            .collect::<Vec<String>>()
            .join(", "),
        embedded.bytes_warning,
        embedded.site_import,
        embedded.user_site_directory,
        embedded.ignore_environment,
        embedded.inspect,
        embedded.interactive,
        embedded.isolated,
        embedded.legacy_windows_fs_encoding,
        embedded.legacy_windows_stdio,
        embedded.write_bytecode,
        embedded.unbuffered_stdio,
        embedded.parser_debug,
        embedded.quiet,
        embedded.use_hash_seed,
        embedded.verbose,
        embedded_resources_path.display(),
        embedded.sys_frozen,
        embedded.sys_meipass,
        match embedded.raw_allocator {
            RawAllocator::Jemalloc => "pyembed::PythonRawAllocator::jemalloc()",
            RawAllocator::Rust => "pyembed::PythonRawAllocator::rust()",
            RawAllocator::System => "pyembed::PythonRawAllocator::system()",
        },
        match embedded.terminfo_resolution {
            TerminfoResolution::Dynamic => "pyembed::TerminfoResolution::Dynamic".to_string(),
            TerminfoResolution::None => "pyembed::TerminfoResolution::None".to_string(),
            TerminfoResolution::Static(ref v) => {
                format!("pyembed::TerminfoResolution::Static(r###\"{}\"###", v)
            }
        },
        match &embedded.write_modules_directory_env {
            Some(path) => "Some(\"".to_owned() + &path + "\".to_string())",
            _ => "None".to_owned(),
        },
        match embedded.run_mode {
            RunMode::Noop => "pyembed::PythonRunMode::None".to_owned(),
            RunMode::Repl => "pyembed::PythonRunMode::Repl".to_owned(),
            RunMode::Module { ref module } => {
                "pyembed::PythonRunMode::Module { module: \"".to_owned()
                    + module
                    + "\".to_string() }"
            }
            RunMode::Eval { ref code } => {
                "pyembed::PythonRunMode::Eval { code: r###\"".to_owned()
                    + code
                    + "\"###.to_string() }"
            }
            RunMode::File { ref path } => {
                "pyembed::PythonRunMode::File { path: std::path::PathBuf::new(r###\"".to_owned()
                    + path
                    + "\"###) }"
            }
        },
    )
}

/// Write a standalone .rs file containing a function for obtaining the default PythonConfig.
pub fn write_default_python_config_rs(path: &Path, python_config_rs: &str) -> Result<()> {
    let mut f = File::create(&path)?;

    // Ideally we would have a const struct, but we need to do some
    // dynamic allocations. Using a function avoids having to pull in a
    // dependency on lazy_static.
    let indented = python_config_rs
        .split('\n')
        .map(|line| "    ".to_owned() + line)
        .join("\n");

    f.write_fmt(format_args!(
        "/// Obtain the default Python configuration\n\
         ///\n\
         /// The crate is compiled with a default Python configuration embedded\n\
         /// in the crate. This function will return an instance of that\n\
         /// configuration.\n\
         pub fn default_python_config<'a>() -> pyembed::PythonConfig<'a> {{\n{}\n}}\n",
        indented
    ))?;

    Ok(())
}
