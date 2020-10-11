// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Configuring a Python interpreter.
*/

use {
    anyhow::Result,
    itertools::Itertools,
    python_packaging::{
        interpreter::{
            BytesWarning, MemoryAllocatorBackend, PythonInterpreterConfig,
            PythonInterpreterProfile, PythonRunMode, TerminfoResolution,
        },
        resource::BytecodeOptimizationLevel,
    },
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

fn optional_bool_to_string(value: &Option<bool>) -> String {
    match value {
        Some(value) => format!("Some({})", value),
        None => "None".to_string(),
    }
}

/// Represents the run-time configuration of a Python interpreter.
///
/// This type mirrors `pyembed::OxidizedPythonInterpreterConfig`. We can't
/// use that type verbatim because of lifetime issues. It might be possible.
/// But that type holds a reference to resources data and this type needs to
/// be embedded in Starlark values, which have a `static lifetime.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddedPythonConfig {
    pub config: PythonInterpreterConfig,
    pub raw_allocator: MemoryAllocatorBackend,
    pub oxidized_importer: bool,
    pub filesystem_importer: bool,
    pub sys_frozen: bool,
    pub sys_meipass: bool,
    pub terminfo_resolution: TerminfoResolution,
    pub write_modules_directory_env: Option<String>,
    pub run_mode: PythonRunMode,
}

impl Default for EmbeddedPythonConfig {
    fn default() -> Self {
        EmbeddedPythonConfig {
            config: PythonInterpreterConfig {
                profile: PythonInterpreterProfile::Isolated,
                buffered_stdio: Some(true),
                bytes_warning: Some(BytesWarning::None),
                inspect: Some(false),
                interactive: Some(false),
                legacy_windows_fs_encoding: Some(false),
                legacy_windows_stdio: Some(false),
                optimization_level: Some(BytecodeOptimizationLevel::Zero),
                parser_debug: Some(false),
                quiet: Some(false),
                site_import: Some(false),
                use_environment: Some(false),
                user_site_directory: Some(false),
                verbose: Some(false),
                write_bytecode: Some(false),
                ..PythonInterpreterConfig::default()
            },
            raw_allocator: MemoryAllocatorBackend::System,
            oxidized_importer: true,
            filesystem_importer: false,
            sys_frozen: false,
            sys_meipass: false,
            terminfo_resolution: TerminfoResolution::None,
            write_modules_directory_env: None,
            run_mode: PythonRunMode::Repl,
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
            configure_locale: {},\n        \
            coerce_c_locale: None,\n        \
            coerce_c_locale_warn: {},\n        \
            development_mode: {},\n        \
            isolated: {},\n        \
            legacy_windows_fs_encoding: {},\n        \
            parse_argv: {},\n        \
            use_environment: {},\n        \
            utf8_mode: {},\n        \
            argv: None,\n        \
            base_exec_prefix: None,\n        \
            base_executable: None,\n        \
            base_prefix: None,\n        \
            buffered_stdio: {},\n        \
            bytes_warning: {},\n        \
            check_hash_pycs_mode: None,\n        \
            configure_c_stdio: {},\n        \
            dump_refs: {},\n        \
            exec_prefix: None,\n        \
            executable: None,\n        \
            fault_handler: {},\n        \
            filesystem_encoding: None,\n        \
            filesystem_errors: None,\n        \
            hash_seed: None,\n        \
            home: None,\n        \
            import_time: {},\n        \
            inspect: {},\n        \
            install_signal_handlers: {},\n        \
            interactive: {},\n        \
            legacy_windows_stdio: {},\n        \
            malloc_stats: {},\n        \
            module_search_paths: {},\n        \
            optimization_level: {},\n        \
            prefix: None,\n        \
            program_name: None,\n        \
            python_path_env: None,\n        \
            parser_debug: {},\n        \
            pathconfig_warnings: {},\n        \
            pycache_prefix: None,\n        \
            quiet: {},\n        \
            run_command: None,\n        \
            run_filename: None,\n        \
            run_module: None,\n        \
            show_alloc_count: {},\n        \
            show_ref_count: {},\n        \
            site_import: {},\n        \
            skip_first_source_line: {},\n        \
            stdio_encoding: {},\n        \
            stdio_errors: {},\n        \
            tracemalloc: {},\n        \
            user_site_directory: {},\n        \
            verbose: {},\n        \
            warn_options: None,\n        \
            write_bytecode: {},\n        \
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
            match self.config.profile {
                PythonInterpreterProfile::Isolated => "pyembed::PythonInterpreterProfile::Isolated",
                PythonInterpreterProfile::Python => "pyembed::PythonInterpreterProfile::Python",
            },
            optional_bool_to_string(&self.config.configure_locale),
            optional_bool_to_string(&self.config.coerce_c_locale_warn),
            optional_bool_to_string(&self.config.development_mode),
            optional_bool_to_string(&self.config.isolated),
            optional_bool_to_string(&self.config.legacy_windows_fs_encoding),
            optional_bool_to_string(&self.config.parse_argv),
            optional_bool_to_string(&self.config.use_environment),
            optional_bool_to_string(&self.config.utf8_mode),
            optional_bool_to_string(&self.config.buffered_stdio),
            match self.config.bytes_warning {
                Some(BytesWarning::None) => "Some(pyembed::BytesWarning::None)",
                Some(BytesWarning::Warn) => "Some(pyembed::BytesWarning::Warn)",
                Some(BytesWarning::Raise) => "Some(pyembed::BytesWarning::Raise)",
                None => "None",
            },
            optional_bool_to_string(&self.config.configure_c_stdio),
            optional_bool_to_string(&self.config.dump_refs),
            optional_bool_to_string(&self.config.fault_handler),
            optional_bool_to_string(&self.config.import_time),
            optional_bool_to_string(&self.config.inspect),
            optional_bool_to_string(&self.config.install_signal_handlers),
            optional_bool_to_string(&self.config.interactive),
            optional_bool_to_string(&self.config.legacy_windows_stdio),
            optional_bool_to_string(&self.config.malloc_stats),
            match &self.config.module_search_paths {
                Some(paths) => {
                    format!(
                        "Some({})",
                        paths
                            .iter()
                            .map(|p| format_args!("\"{}\"", p.display()).to_string())
                            .collect::<Vec<String>>()
                            .join(", ")
                    )
                }
                None => "None".to_string(),
            },
            match self.config.optimization_level {
                Some(BytecodeOptimizationLevel::Zero) =>
                    "Some(pyembed::BytecodeOptimizationLevel::Zero)",
                Some(BytecodeOptimizationLevel::One) =>
                    "Some(pyembed::BytecodeOptimizationLevel::One)",
                Some(BytecodeOptimizationLevel::Two) =>
                    "Some(pyembed::BytecodeOptimizationLevel::Two)",
                None => "None",
            },
            optional_bool_to_string(&self.config.parser_debug),
            optional_bool_to_string(&self.config.pathconfig_warnings),
            optional_bool_to_string(&self.config.quiet),
            optional_bool_to_string(&self.config.show_alloc_count),
            optional_bool_to_string(&self.config.show_ref_count),
            optional_bool_to_string(&self.config.site_import),
            optional_bool_to_string(&self.config.skip_first_source_line),
            match &self.config.stdio_encoding {
                Some(value) => format_args!("Some(\"{}\")", value).to_string(),
                None => "None".to_string(),
            },
            match &self.config.stdio_errors {
                Some(value) => format_args!("Some(\"{}\")", value).to_string(),
                None => "None".to_owned(),
            },
            optional_bool_to_string(&self.config.tracemalloc),
            optional_bool_to_string(&self.config.user_site_directory),
            optional_bool_to_string(&self.config.verbose),
            optional_bool_to_string(&self.config.write_bytecode),
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
