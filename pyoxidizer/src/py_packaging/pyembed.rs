// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Functionality related to the pyembed crate.
*/

use {
    super::config::{EmbeddedPythonConfig, RunMode},
    anyhow::Result,
    itertools::Itertools,
    python_packaging::interpreter::{MemoryAllocatorBackend, TerminfoResolution},
    std::{
        fs::File,
        io::Write,
        path::{Path, PathBuf},
    },
};

/// Obtain the Rust source code to construct a OxidizedPythonInterpreterConfig instance.
pub fn derive_python_config(
    embedded: &EmbeddedPythonConfig,
    embedded_resources_path: &PathBuf,
) -> String {
    format!(
        "pyembed::OxidizedPythonInterpreterConfig {{\n    \
        interpreter_config: pyembed::PythonInterpreterConfig {{\n        \
        profile: {},\n        \
        allocator: None,\n        \
        configure_locale: None,\n        \
        coerce_c_locale: None,\n        \
        coerce_c_locale_warn: None,\n        \
        development_mode: None,\n        \
        isolated: None,\n        \
        legacy_windows_fs_encoding: Some({}),\n        \
        parse_argv: None,\n        \
        use_environment: Some({}),\n        \
        utf8_mode: None,\n        \
        argv: None,\n        \
        base_exec_prefix: None,\n        \
        base_executable: None,\n        \
        base_prefix: None,\n        \
        buffered_stdio: Some({}),\n        \
        bytes_warning: Some({}),\n        \
        check_hash_pycs_mode: None,\n        \
        configure_c_stdio: None,\n        \
        dump_refs: None,\n        \
        exec_prefix: None,\n        \
        executable: None,\n        \
        fault_handler: None,\n        \
        filesystem_encoding: None,\n        \
        filesystem_errors: None,\n        \
        hash_seed: None,\n        \
        home: None,\n        \
        import_time: None,\n        \
        install_signal_handlers: None,\n        \
        inspect: Some({}),\n        \
        interactive: Some({}),\n        \
        legacy_windows_stdio: Some({}),\n        \
        malloc_stats: None,\n        \
        module_search_paths: {},\n        \
        optimization_level: Some({}),\n        \
        prefix: None,\n        \
        program_name: None,\n        \
        python_path_env: None,\n        \
        parser_debug: Some({}),\n        \
        pathconfig_warnings: None,\n        \
        pycache_prefix: None,\n        \
        quiet: Some({}),\n        \
        run_command: None,\n        \
        run_filename: None,\n        \
        run_module: None,\n        \
        show_alloc_count: None,\n        \
        show_ref_count: None,\n        \
        skip_first_source_line: None,\n        \
        site_import: Some({}),\n        \
        stdio_encoding: {},\n        \
        stdio_errors: {},\n        \
        tracemalloc: None,\n        \
        user_site_directory: Some({}),\n        \
        verbose: Some({}),\n        \
        warn_options: None,\n        \
        write_bytecode: Some({}),\n        \
        x_options: None,\n        \
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
        embedded.legacy_windows_fs_encoding,
        !embedded.ignore_environment,
        !embedded.unbuffered_stdio,
        match embedded.bytes_warning {
            0 => "pyembed::BytesWarning::None",
            1 => "pyembed::BytesWarning::Warn",
            2 => "pyembed::BytesWarning::Raise",
            _ => "pyembed::BytesWarning::Raise",
        },
        embedded.inspect,
        embedded.interactive,
        embedded.legacy_windows_stdio,
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
        match embedded.optimize_level {
            0 => "pyembed::OptimizationLevel::Zero",
            1 => "pyembed::OptimizationLevel::One",
            2 => "pyembed::OptimizationLevel::Two",
            _ => "pyembed::OptimizationLevel::Two",
        },
        embedded.parser_debug,
        embedded.quiet,
        embedded.site_import,
        match &embedded.stdio_encoding_name {
            Some(value) => format_args!("Some(\"{}\")", value).to_string(),
            None => "None".to_owned(),
        },
        match &embedded.stdio_encoding_errors {
            Some(value) => format_args!("Some(\"{}\")", value).to_string(),
            None => "None".to_owned(),
        },
        embedded.user_site_directory,
        embedded.verbose != 0,
        embedded.write_bytecode,
        match embedded.raw_allocator {
            MemoryAllocatorBackend::Jemalloc => "pyembed::PythonRawAllocator::jemalloc()",
            MemoryAllocatorBackend::Rust => "pyembed::PythonRawAllocator::rust()",
            MemoryAllocatorBackend::System => "pyembed::PythonRawAllocator::system()",
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

/// Write a standalone .rs file containing a function for obtaining the default OxidizedPythonInterpreterConfig.
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
