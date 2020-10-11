// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Configuring a Python interpreter.
*/

use {
    anyhow::Result,
    itertools::Itertools,
    python_packaging::interpreter::{MemoryAllocatorBackend, PythonRunMode, TerminfoResolution},
    std::{io::Write, path::Path},
};

/// Determine the default raw allocator for a target triple.
pub fn default_raw_allocator(target_triple: &str) -> MemoryAllocatorBackend {
    // Jemalloc doesn't work on Windows.
    //
    // We don't use Jemalloc by default in the test environment because it slows down
    // builds of test projects.
    if target_triple == "x86_64-pc-windows-msvc" || cfg!(test) {
        MemoryAllocatorBackend::System
    } else {
        MemoryAllocatorBackend::Jemalloc
    }
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
    pub raw_allocator: MemoryAllocatorBackend,
    pub run_mode: PythonRunMode,
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
            raw_allocator: MemoryAllocatorBackend::System,
            run_mode: PythonRunMode::Repl,
            terminfo_resolution: TerminfoResolution::None,
            user_site_directory: false,
            write_bytecode: false,
            write_modules_directory_env: None,
        }
    }
}

impl EmbeddedPythonConfig {
    /// Convert the instance to Rust code that constructs a `pyembed::OxidizedPythonInterpreterConfig`.
    pub fn to_oxidized_python_interpreter_config_rs(
        &self,
        packed_resources_path: Option<&Path>,
    ) -> Result<String> {
        let code = format!(
            "pyembed::OxidizedPythonInterpreterConfig {{\n    \
            origin: None,\n    \
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
            packed_resources: {},\n    \
            extra_extension_modules: None,\n    \
            argvb: false,\n    \
            sys_frozen: {},\n    \
            sys_meipass: {},\n    \
            terminfo_resolution: {},\n    \
            write_modules_directory_env: {},\n    \
            run: {},\n\
            }}\n\
            ",
            if self.isolated {
                "pyembed::PythonInterpreterProfile::Isolated"
            } else {
                "pyembed::PythonInterpreterProfile::Python"
            },
            self.legacy_windows_fs_encoding,
            !self.ignore_environment,
            !self.unbuffered_stdio,
            match self.bytes_warning {
                0 => "pyembed::BytesWarning::None",
                1 => "pyembed::BytesWarning::Warn",
                2 => "pyembed::BytesWarning::Raise",
                _ => "pyembed::BytesWarning::Raise",
            },
            self.inspect,
            self.interactive,
            self.legacy_windows_stdio,
            if self.sys_paths.is_empty() {
                "None".to_string()
            } else {
                format!(
                    "Some({})",
                    &self
                        .sys_paths
                        .iter()
                        .map(|p| "\"".to_owned() + p + "\".to_string()")
                        .collect::<Vec<String>>()
                        .join(", ")
                )
            },
            match self.optimize_level {
                0 => "pyembed::BytecodeOptimizationLevel::Zero",
                1 => "pyembed::BytecodeOptimizationLevel::One",
                2 => "pyembed::BytecodeOptimizationLevel::Two",
                _ => "pyembed::BytecodeOptimizationLevel::Two",
            },
            self.parser_debug,
            self.quiet,
            self.site_import,
            match &self.stdio_encoding_name {
                Some(value) => format_args!("Some(\"{}\")", value).to_string(),
                None => "None".to_owned(),
            },
            match &self.stdio_encoding_errors {
                Some(value) => format_args!("Some(\"{}\")", value).to_string(),
                None => "None".to_owned(),
            },
            self.user_site_directory,
            self.verbose != 0,
            self.write_bytecode,
            match self.raw_allocator {
                MemoryAllocatorBackend::Jemalloc => "pyembed::PythonRawAllocator::jemalloc()",
                MemoryAllocatorBackend::Rust => "pyembed::PythonRawAllocator::rust()",
                MemoryAllocatorBackend::System => "pyembed::PythonRawAllocator::system()",
            },
            self.filesystem_importer,
            if let Some(path) = packed_resources_path {
                format!("Some(include_bytes!(r#\"{}\"#))", path.display())
            } else {
                "None".to_string()
            },
            self.sys_frozen,
            self.sys_meipass,
            match self.terminfo_resolution {
                TerminfoResolution::Dynamic => "pyembed::TerminfoResolution::Dynamic".to_string(),
                TerminfoResolution::None => "pyembed::TerminfoResolution::None".to_string(),
                TerminfoResolution::Static(ref v) => {
                    format!("pyembed::TerminfoResolution::Static(r###\"{}\"###", v)
                }
            },
            match &self.write_modules_directory_env {
                Some(path) => "Some(\"".to_owned() + &path + "\".to_string())",
                _ => "None".to_owned(),
            },
            match self.run_mode {
                PythonRunMode::None => "pyembed::PythonRunMode::None".to_owned(),
                PythonRunMode::Repl => "pyembed::PythonRunMode::Repl".to_owned(),
                PythonRunMode::Module { ref module } => {
                    "pyembed::PythonRunMode::Module { module: \"".to_owned()
                        + module
                        + "\".to_string() }"
                }
                PythonRunMode::Eval { ref code } => {
                    "pyembed::PythonRunMode::Eval { code: r###\"".to_owned()
                        + code
                        + "\"###.to_string() }"
                }
                PythonRunMode::File { ref path } => {
                    format!("pyembed::PythonRunMode::File {{ path: std::path::PathBuf::new(r###\"{}\"###) }}",
                    path.display())
                }
            },
        );

        Ok(code)
    }

    /// Write a Rust file containing a function for obtaining the default `OxidizedPythonInterpreterConfig`.
    pub fn write_default_python_confis_rs(
        &self,
        path: &Path,
        packed_resources_path: Option<&Path>,
    ) -> Result<()> {
        let mut f = std::fs::File::create(&path)?;

        let indented = self
            .to_oxidized_python_interpreter_config_rs(packed_resources_path)?
            .split('\n')
            .map(|line| "    ".to_string() + line)
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
}
