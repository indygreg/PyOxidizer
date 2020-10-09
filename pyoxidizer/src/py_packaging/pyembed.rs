// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Functionality related to the pyembed crate.
*/

use {
    anyhow::Result,
    itertools::Itertools,
    std::{
        fs::File,
        io::Write,
        path::{Path, PathBuf},
    },
};

use super::config::{EmbeddedPythonConfig, RawAllocator, RunMode, TerminfoResolution};

/// Obtain the Rust source code to construct a PythonConfig instance.
pub fn derive_python_config(
    embedded: &EmbeddedPythonConfig,
    embedded_resources_path: &PathBuf,
) -> String {
    format!(
        "pyembed::OxidizedPythonInterpreterConfig {{\n    \
        interpreter_config: pyembed::PythonInterpreterConfig {{\n        \
        profile: {},\n        \
        stdio_encoding: {},\n        \
        stdio_errors: {},\n        \
        optimization_level: Some({}),\n        \
        module_search_paths: {},\n        \
        bytes_warning: Some({}),\n        \
        site_import: Some({}),\n        \
        user_site_directory: Some({}),\n        \
        use_environment: Some({}),\n        \
        inspect: Some({}),\n        \
        interactive: Some({}),\n        \
        legacy_windows_fs_encoding: Some({}),\n        \
        legacy_windows_stdio: Some({}),\n        \
        write_bytecode: Some({}),\n        \
        buffered_stdio: Some({}),\n        \
        parser_debug: Some({}),\n        \
        quiet: Some({}),\n        \
        verbose: Some({}),\n        \
        ..pyembed::PythonInterpreterConfig::default()\n    \
        }},\n    \
        raw_allocator: Some({}),\n    \
        oxidized_importer: true,\n    \
        filesystem_importer: {},\n    \
        packed_resources: Some(include_bytes!(r#\"{}\"#)),\n    \
        extra_extension_modules: None,\n    \
        argvb: false,\n    \
        sys_frozen: {},\n    \
        sys_meipass: {},\n    \
        terminfo_resolution: {},\n    \
        write_modules_directory_env: {},\n    \
        run: {},\n\
        }}\n",
        if embedded.isolated {
            "pyembed::PythonInterpreterProfile::Isolated"
        } else {
            "pyembed::PythonInterpreterProfile::Python"
        },
        match &embedded.stdio_encoding_name {
            Some(value) => format_args!("Some(\"{}\")", value).to_string(),
            None => "None".to_owned(),
        },
        match &embedded.stdio_encoding_errors {
            Some(value) => format_args!("Some(\"{}\")", value).to_string(),
            None => "None".to_owned(),
        },
        match embedded.optimize_level {
            0 => "pyembed::OptimizationLevel::Zero",
            1 => "pyembed::OptimizationLevel::One",
            2 => "pyembed::OptimizationLevel::Two",
            _ => "pyembed::OptimizationLevel::Two",
        },
        if embedded.sys_paths.is_empty() {
            "None".to_string()
        } else {
            format!(
                "Some({})",
                &embedded
                    .sys_paths
                    .iter()
                    .map(|p| "\"".to_owned() + p + "\".to_string()")
                    .collect::<Vec<String>>()
                    .join(", ")
            )
        },
        match embedded.bytes_warning {
            0 => "pyembed::BytesWarning::None",
            1 => "pyembed::BytesWarning::Warn",
            2 => "pyembed::BytesWarning::Raise",
            _ => "pyembed::BytesWarning::Raise",
        },
        embedded.site_import,
        embedded.user_site_directory,
        !embedded.ignore_environment,
        embedded.inspect,
        embedded.interactive,
        embedded.legacy_windows_fs_encoding,
        embedded.legacy_windows_stdio,
        embedded.write_bytecode,
        !embedded.unbuffered_stdio,
        embedded.parser_debug,
        embedded.quiet,
        embedded.verbose != 0,
        match embedded.raw_allocator {
            RawAllocator::Jemalloc => "pyembed::PythonRawAllocator::jemalloc()",
            RawAllocator::Rust => "pyembed::PythonRawAllocator::rust()",
            RawAllocator::System => "pyembed::PythonRawAllocator::system()",
        },
        embedded.filesystem_importer,
        embedded_resources_path.display(),
        embedded.sys_frozen,
        embedded.sys_meipass,
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
         pub fn default_python_config<'a>() -> pyembed::OxidizedPythonInterpreterConfig<'a> {{\n{}\n}}\n",
        indented
    ))?;

    Ok(())
}
