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
            Allocator, BytesWarning, CheckHashPycsMode, CoerceCLocale, MemoryAllocatorBackend,
            MultiprocessingStartMethod, PythonInterpreterConfig, PythonInterpreterProfile,
            TerminfoResolution,
        },
        resource::BytecodeOptimizationLevel,
    },
    std::{
        io::Write,
        path::{Path, PathBuf},
    },
};

/// Determine the default memory allocator for a target triple.
pub fn default_memory_allocator(target_triple: &str) -> MemoryAllocatorBackend {
    // Jemalloc doesn't work on Windows.
    //
    // We don't use Jemalloc by default in the test environment because it slows down
    // builds of test projects.
    if target_triple.ends_with("-pc-windows-msvc") || cfg!(test) {
        MemoryAllocatorBackend::Default
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

fn optional_string_to_string(value: &Option<String>) -> String {
    match value {
        Some(value) => format!("Some(\"{}\".to_string())", value.escape_default()),
        None => "None".to_string(),
    }
}

fn path_to_string(value: &Path) -> String {
    format!(
        "std::path::PathBuf::from(\"{}\")",
        value.display().to_string().escape_default()
    )
}

fn optional_pathbuf_to_string(value: &Option<PathBuf>) -> String {
    match value {
        Some(value) => format!("Some({})", path_to_string(value)),
        None => "None".to_string(),
    }
}

fn optional_vec_string_to_string(value: &Option<Vec<String>>) -> String {
    match value {
        Some(value) => format!(
            "Some(vec![{}])",
            value
                .iter()
                .map(|x| format!("\"{}\".to_string()", x.escape_default()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => "None".to_string(),
    }
}

/// Represents sources for loading packed resources data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PyembedPackedResourcesSource {
    /// Load from memory via an `include_bytes!` directive.
    MemoryIncludeBytes(PathBuf),
    /// Load from a file using memory mapped I/O.
    ///
    /// The string `$ORIGIN` is expanded at runtime.
    MemoryMappedPath(PathBuf),
}

impl ToString for PyembedPackedResourcesSource {
    fn to_string(&self) -> String {
        match self {
            Self::MemoryIncludeBytes(path) => {
                format!(
                    "pyembed::PackedResourcesSource::Memory(include_bytes!(r#\"{}\"#))",
                    path.display()
                )
            }
            Self::MemoryMappedPath(path) => {
                format!(
                    "pyembed::PackedResourcesSource::MemoryMappedPath({})",
                    path_to_string(path)
                )
            }
        }
    }
}

/// Represents the run-time configuration of a Python interpreter.
///
/// This type mirrors `pyembed::OxidizedPythonInterpreterConfig`. We can't
/// use that type verbatim because of lifetime issues. It might be possible.
/// But that type holds a reference to resources data and this type needs to
/// be embedded in Starlark values, which have a `static lifetime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyembedPythonInterpreterConfig {
    pub config: PythonInterpreterConfig,
    pub allocator_backend: MemoryAllocatorBackend,
    pub allocator_raw: bool,
    pub allocator_mem: bool,
    pub allocator_obj: bool,
    pub allocator_pymalloc_arena: bool,
    pub allocator_debug: bool,
    pub set_missing_path_configuration: bool,
    pub oxidized_importer: bool,
    pub filesystem_importer: bool,
    pub packed_resources: Vec<PyembedPackedResourcesSource>,
    pub argvb: bool,
    pub multiprocessing_auto_dispatch: bool,
    pub multiprocessing_start_method: MultiprocessingStartMethod,
    pub sys_frozen: bool,
    pub sys_meipass: bool,
    pub terminfo_resolution: TerminfoResolution,
    pub tcl_library: Option<PathBuf>,
    pub write_modules_directory_env: Option<String>,
}

impl Default for PyembedPythonInterpreterConfig {
    fn default() -> Self {
        PyembedPythonInterpreterConfig {
            config: PythonInterpreterConfig {
                profile: PythonInterpreterProfile::Isolated,
                // Isolated mode disables configure_locale by default. But this
                // setting is essential for properly initializing encoding at
                // run-time. Without this, UTF-8 arguments are mangled, for
                // example. See
                // https://github.com/indygreg/PyOxidizer/issues/294 for more.
                configure_locale: Some(true),
                ..PythonInterpreterConfig::default()
            },
            allocator_backend: MemoryAllocatorBackend::Default,
            // This setting has no effect by itself. But the default of true
            // makes it so a custom backend is used automatically.
            allocator_raw: true,
            allocator_mem: false,
            allocator_obj: false,
            allocator_pymalloc_arena: false,
            allocator_debug: false,
            set_missing_path_configuration: true,
            oxidized_importer: true,
            filesystem_importer: false,
            packed_resources: vec![],
            argvb: false,
            multiprocessing_auto_dispatch: true,
            multiprocessing_start_method: MultiprocessingStartMethod::Auto,
            sys_frozen: true,
            sys_meipass: false,
            terminfo_resolution: TerminfoResolution::None,
            tcl_library: None,
            write_modules_directory_env: None,
        }
    }
}

impl PyembedPythonInterpreterConfig {
    /// Convert the instance to Rust code that constructs a `pyembed::OxidizedPythonInterpreterConfig`.
    pub fn to_oxidized_python_interpreter_config_rs(&self) -> Result<String> {
        // This code is complicated enough. Let's not worry about format! in format!.
        #[allow(unknown_lints, clippy::format_in_format_args)]
        let code = format!(
            "pyembed::OxidizedPythonInterpreterConfig {{\n    \
            exe: None,\n    \
            origin: None,\n    \
            interpreter_config: pyembed::PythonInterpreterConfig {{\n        \
            profile: {},\n        \
            allocator: {},\n        \
            configure_locale: {},\n        \
            coerce_c_locale: {},\n        \
            coerce_c_locale_warn: {},\n        \
            development_mode: {},\n        \
            isolated: {},\n        \
            legacy_windows_fs_encoding: {},\n        \
            parse_argv: {},\n        \
            use_environment: {},\n        \
            utf8_mode: {},\n        \
            argv: None,\n        \
            base_exec_prefix: {},\n        \
            base_executable: {},\n        \
            base_prefix: {},\n        \
            buffered_stdio: {},\n        \
            bytes_warning: {},\n        \
            check_hash_pycs_mode: {},\n        \
            configure_c_stdio: {},\n        \
            dump_refs: {},\n        \
            exec_prefix: {},\n        \
            executable: {},\n        \
            fault_handler: {},\n        \
            filesystem_encoding: {},\n        \
            filesystem_errors: {},\n        \
            hash_seed: {},\n        \
            home: {},\n        \
            import_time: {},\n        \
            inspect: {},\n        \
            install_signal_handlers: {},\n        \
            interactive: {},\n        \
            legacy_windows_stdio: {},\n        \
            malloc_stats: {},\n        \
            module_search_paths: {},\n        \
            optimization_level: {},\n        \
            parser_debug: {},\n        \
            pathconfig_warnings: {},\n        \
            prefix: {},\n        \
            program_name: {},\n        \
            pycache_prefix: {},\n        \
            python_path_env: {},\n        \
            quiet: {},\n        \
            run_command: {},\n        \
            run_filename: {},\n        \
            run_module: {},\n        \
            show_ref_count: {},\n        \
            site_import: {},\n        \
            skip_first_source_line: {},\n        \
            stdio_encoding: {},\n        \
            stdio_errors: {},\n        \
            tracemalloc: {},\n        \
            user_site_directory: {},\n        \
            verbose: {},\n        \
            warn_options: {},\n        \
            write_bytecode: {},\n        \
            x_options: {},\n        \
            }},\n    \
            allocator_backend: {},\n    \
            allocator_raw: {},\n    \
            allocator_mem: {},\n    \
            allocator_obj: {},\n    \
            allocator_pymalloc_arena: {},\n    \
            allocator_debug: {},\n    \
            set_missing_path_configuration: {},\n    \
            oxidized_importer: {},\n    \
            filesystem_importer: {},\n    \
            packed_resources: {},\n    \
            extra_extension_modules: None,\n    \
            argv: None,\n    \
            argvb: {},\n    \
            multiprocessing_auto_dispatch: {},\n    \
            multiprocessing_start_method: {},\n    \
            sys_frozen: {},\n    \
            sys_meipass: {},\n    \
            terminfo_resolution: {},\n    \
            tcl_library: {},\n    \
            write_modules_directory_env: {},\n    \
            }}\n\
            ",
            match self.config.profile {
                PythonInterpreterProfile::Isolated => "pyembed::PythonInterpreterProfile::Isolated",
                PythonInterpreterProfile::Python => "pyembed::PythonInterpreterProfile::Python",
            },
            match self.config.allocator {
                Some(Allocator::Debug) => "Some(pyembed::Allocator::Debug)",
                Some(Allocator::Default) => "Some(pyembed::Allocator::Default)",
                Some(Allocator::Malloc) => "Some(pyembed::Allocator::Malloc)",
                Some(Allocator::MallocDebug) => "Some(pyembed::Allocator::MallocDebug)",
                Some(Allocator::NotSet) => "Some(pyembed::Allocator::NotSet)",
                Some(Allocator::PyMalloc) => "Some(pyembed::Allocator::PyMalloc)",
                Some(Allocator::PyMallocDebug) => "Some(pyembed::Allocator::PyMallocDebug)",
                None => "None",
            },
            optional_bool_to_string(&self.config.configure_locale),
            match &self.config.coerce_c_locale {
                Some(CoerceCLocale::C) => "Some(pyembed::CoerceCLocale::C)",
                Some(CoerceCLocale::LCCtype) => "Some(pyembed::CoerceCLocale::LCCtype)",
                None => "None",
            },
            optional_bool_to_string(&self.config.coerce_c_locale_warn),
            optional_bool_to_string(&self.config.development_mode),
            optional_bool_to_string(&self.config.isolated),
            optional_bool_to_string(&self.config.legacy_windows_fs_encoding),
            optional_bool_to_string(&self.config.parse_argv),
            optional_bool_to_string(&self.config.use_environment),
            optional_bool_to_string(&self.config.utf8_mode),
            optional_pathbuf_to_string(&self.config.base_exec_prefix),
            optional_pathbuf_to_string(&self.config.base_executable),
            optional_pathbuf_to_string(&self.config.base_prefix),
            optional_bool_to_string(&self.config.buffered_stdio),
            match self.config.bytes_warning {
                Some(BytesWarning::None) => "Some(pyembed::BytesWarning::None)",
                Some(BytesWarning::Warn) => "Some(pyembed::BytesWarning::Warn)",
                Some(BytesWarning::Raise) => "Some(pyembed::BytesWarning::Raise)",
                None => "None",
            },
            match self.config.check_hash_pycs_mode {
                Some(CheckHashPycsMode::Always) => "Some(pyembed::CheckHashPycsMode::Always)",
                Some(CheckHashPycsMode::Default) => "Some(pyembed::CheckHashPycsMode::Default)",
                Some(CheckHashPycsMode::Never) => "Some(pyembed::CheckHashPycsMode::Never)",
                None => "None",
            },
            optional_bool_to_string(&self.config.configure_c_stdio),
            optional_bool_to_string(&self.config.dump_refs),
            optional_pathbuf_to_string(&self.config.exec_prefix),
            optional_pathbuf_to_string(&self.config.executable),
            optional_bool_to_string(&self.config.fault_handler),
            optional_string_to_string(&self.config.filesystem_encoding),
            optional_string_to_string(&self.config.filesystem_errors),
            match &self.config.hash_seed {
                Some(value) => format!("Some({})", value),
                None => "None".to_string(),
            },
            optional_pathbuf_to_string(&self.config.home),
            optional_bool_to_string(&self.config.import_time),
            optional_bool_to_string(&self.config.inspect),
            optional_bool_to_string(&self.config.install_signal_handlers),
            optional_bool_to_string(&self.config.interactive),
            optional_bool_to_string(&self.config.legacy_windows_stdio),
            optional_bool_to_string(&self.config.malloc_stats),
            match &self.config.module_search_paths {
                Some(paths) => {
                    format!(
                        "Some(vec![{}])",
                        paths
                            .iter()
                            .map(|p| path_to_string(p.as_path()))
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
            optional_pathbuf_to_string(&self.config.prefix),
            optional_pathbuf_to_string(&self.config.program_name),
            optional_pathbuf_to_string(&self.config.pycache_prefix),
            optional_string_to_string(&self.config.python_path_env),
            optional_bool_to_string(&self.config.quiet),
            optional_string_to_string(&self.config.run_command),
            optional_pathbuf_to_string(&self.config.run_filename),
            optional_string_to_string(&self.config.run_module),
            optional_bool_to_string(&self.config.show_ref_count),
            optional_bool_to_string(&self.config.site_import),
            optional_bool_to_string(&self.config.skip_first_source_line),
            optional_string_to_string(&self.config.stdio_encoding),
            optional_string_to_string(&self.config.stdio_errors),
            optional_bool_to_string(&self.config.tracemalloc),
            optional_bool_to_string(&self.config.user_site_directory),
            optional_bool_to_string(&self.config.verbose),
            optional_vec_string_to_string(&self.config.warn_options),
            optional_bool_to_string(&self.config.write_bytecode),
            optional_vec_string_to_string(&self.config.x_options),
            match self.allocator_backend {
                MemoryAllocatorBackend::Jemalloc => "pyembed::MemoryAllocatorBackend::Jemalloc",
                MemoryAllocatorBackend::Mimalloc => "pyembed::MemoryAllocatorBackend::Mimalloc",
                MemoryAllocatorBackend::Snmalloc => "pyembed::MemoryAllocatorBackend::Snmalloc",
                MemoryAllocatorBackend::Rust => "pyembed::MemoryAllocatorBackend::Rust",
                MemoryAllocatorBackend::Default => "pyembed::MemoryAllocatorBackend::Default",
            },
            self.allocator_raw,
            self.allocator_mem,
            self.allocator_obj,
            self.allocator_pymalloc_arena,
            self.allocator_debug,
            self.set_missing_path_configuration,
            self.oxidized_importer,
            self.filesystem_importer,
            format!(
                "vec![{}]",
                self.packed_resources
                    .iter()
                    .map(|e| e.to_string())
                    .join(", ")
            ),
            self.argvb,
            self.multiprocessing_auto_dispatch,
            match self.multiprocessing_start_method {
                MultiprocessingStartMethod::None =>
                    "pyembed::MultiprocessingStartMethod::None".to_string(),
                MultiprocessingStartMethod::Fork =>
                    "pyembed::MultiprocessingStartMethod::Fork".to_string(),
                MultiprocessingStartMethod::ForkServer =>
                    "pyembed::MultiprocessingStartMethod::ForkServer".to_string(),
                MultiprocessingStartMethod::Spawn =>
                    "pyembed::MultiprocessingStartMethod::Spawn".to_string(),
                MultiprocessingStartMethod::Auto =>
                    "pyembed::MultiprocessingStartMethod::Auto".to_string(),
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
            optional_pathbuf_to_string(&self.tcl_library),
            optional_string_to_string(&self.write_modules_directory_env),
        );

        Ok(code)
    }

    /// Write a Rust file containing a function for obtaining the default `OxidizedPythonInterpreterConfig`.
    pub fn write_default_python_config_rs(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut f = std::fs::File::create(path.as_ref())?;

        let indented = self
            .to_oxidized_python_interpreter_config_rs()?
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

#[cfg(test)]
mod tests {
    use crate::{
        environment::default_target_triple,
        py_packaging::distribution::{BinaryLibpythonLinkMode, PythonDistribution},
    };
    use {super::*, crate::testutil::*};

    fn assert_contains(haystack: &str, needle: &str) -> Result<()> {
        assert!(
            haystack.contains(needle),
            "expected to find {} in {}",
            needle,
            haystack
        );

        Ok(())
    }

    fn assert_serialize_module_search_paths(paths: &[&str], expected_contents: &str) -> Result<()> {
        let mut config = PyembedPythonInterpreterConfig::default();
        config.config.module_search_paths = Some(paths.iter().map(PathBuf::from).collect());

        let code = config.to_oxidized_python_interpreter_config_rs()?;
        assert_contains(&code, expected_contents)
    }

    #[test]
    fn test_serialize_module_search_paths() -> Result<()> {
        assert_serialize_module_search_paths(
            &["$ORIGIN/lib", "lib"],
            "module_search_paths: Some(vec![std::path::PathBuf::from(\"$ORIGIN/lib\"), std::path::PathBuf::from(\"lib\")]),"
        )
    }

    #[test]
    fn test_serialize_module_search_paths_backslash() -> Result<()> {
        assert_serialize_module_search_paths(
            &["$ORIGIN\\lib", "lib"],
            "module_search_paths: Some(vec![std::path::PathBuf::from(\"$ORIGIN\\\\lib\"), std::path::PathBuf::from(\"lib\")]),"
        )
    }

    #[test]
    fn test_serialize_filesystem_fields() -> Result<()> {
        let mut config = PyembedPythonInterpreterConfig::default();
        config.config.filesystem_encoding = Some("ascii".to_string());
        config.config.filesystem_errors = Some("strict".to_string());

        let code = config.to_oxidized_python_interpreter_config_rs()?;

        assert!(code.contains("filesystem_encoding: Some(\"ascii\".to_string()),"));
        assert!(code.contains("filesystem_errors: Some(\"strict\".to_string()),"));

        Ok(())
    }

    #[test]
    fn test_backslash_in_path() -> Result<()> {
        let config = PyembedPythonInterpreterConfig {
            tcl_library: Some(PathBuf::from("c:\\windows")),
            ..Default::default()
        };

        let code = config.to_oxidized_python_interpreter_config_rs()?;

        assert_contains(
            &code,
            "tcl_library: Some(std::path::PathBuf::from(\"c:\\\\windows\")),",
        )
    }

    // TODO enable once CI has a linkable Python.
    #[test]
    #[ignore]
    fn test_build_all_fields() -> Result<()> {
        let env = get_env()?;
        let dist = get_default_distribution(None)?;
        let policy = dist.create_packaging_policy()?;

        let config = PyembedPythonInterpreterConfig {
            config: PythonInterpreterConfig {
                profile: Default::default(),
                allocator: Some(Allocator::MallocDebug),
                configure_locale: Some(true),
                coerce_c_locale: Some(CoerceCLocale::C),
                coerce_c_locale_warn: Some(true),
                development_mode: Some(true),
                isolated: Some(false),
                legacy_windows_fs_encoding: Some(false),
                parse_argv: Some(true),
                use_environment: Some(true),
                utf8_mode: Some(true),
                argv: Some(vec!["foo".into(), "bar".into()]),
                base_exec_prefix: Some("path".into()),
                base_executable: Some("path".into()),
                base_prefix: Some("path".into()),
                buffered_stdio: Some(false),
                bytes_warning: Some(BytesWarning::Raise),
                check_hash_pycs_mode: Some(CheckHashPycsMode::Always),
                configure_c_stdio: Some(true),
                dump_refs: Some(true),
                exec_prefix: Some("path".into()),
                executable: Some("path".into()),
                fault_handler: Some(false),
                filesystem_encoding: Some("encoding".into()),
                filesystem_errors: Some("errors".into()),
                hash_seed: Some(42),
                home: Some("home".into()),
                import_time: Some(true),
                inspect: Some(false),
                install_signal_handlers: Some(true),
                interactive: Some(true),
                legacy_windows_stdio: Some(false),
                malloc_stats: Some(false),
                module_search_paths: Some(vec!["lib".into()]),
                optimization_level: Some(BytecodeOptimizationLevel::One),
                parser_debug: Some(true),
                pathconfig_warnings: Some(false),
                prefix: Some("prefix".into()),
                program_name: Some("program_name".into()),
                pycache_prefix: Some("prefix".into()),
                python_path_env: Some("env".into()),
                quiet: Some(true),
                run_command: Some("command".into()),
                run_filename: Some("filename".into()),
                run_module: Some("module".into()),
                show_ref_count: Some(false),
                site_import: Some(true),
                skip_first_source_line: Some(false),
                stdio_encoding: Some("encoding".into()),
                stdio_errors: Some("errors".into()),
                tracemalloc: Some(false),
                user_site_directory: Some(false),
                verbose: Some(true),
                warn_options: Some(vec!["option0".into(), "option1".into()]),
                write_bytecode: Some(true),
                x_options: Some(vec!["x0".into(), "x1".into()]),
            },
            allocator_backend: MemoryAllocatorBackend::Default,
            allocator_raw: true,
            allocator_mem: true,
            allocator_obj: true,
            allocator_pymalloc_arena: true,
            allocator_debug: true,
            set_missing_path_configuration: false,
            oxidized_importer: true,
            filesystem_importer: true,
            packed_resources: vec![
                PyembedPackedResourcesSource::MemoryIncludeBytes(PathBuf::from("packed-resources")),
                PyembedPackedResourcesSource::MemoryMappedPath(PathBuf::from(
                    "$ORIGIN/packed-resources",
                )),
            ],
            argvb: true,
            sys_frozen: false,
            sys_meipass: true,
            terminfo_resolution: TerminfoResolution::Dynamic,
            tcl_library: Some("path".into()),
            write_modules_directory_env: Some("env".into()),
            multiprocessing_auto_dispatch: false,
            multiprocessing_start_method: MultiprocessingStartMethod::Spawn,
        };

        let builder = dist.as_python_executable_builder(
            default_target_triple(),
            default_target_triple(),
            "all_config_fields",
            BinaryLibpythonLinkMode::Dynamic,
            &policy,
            &config,
            None,
        )?;

        crate::project_building::build_python_executable(
            &env,
            "all_config_fields",
            builder.as_ref(),
            default_target_triple(),
            "0",
            false,
        )?;

        Ok(())
    }
}
