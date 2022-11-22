// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    once_cell::sync::Lazy,
    pyembed::{MainPythonInterpreter, OxidizedPythonInterpreterConfig, PackedResourcesSource},
    pyo3::{prelude::*, types::PyBytes},
    pyoxidizerlib::{
        environment::{default_target_triple, Environment},
        py_packaging::{
            distribution::{DistributionCache, DistributionFlavor, PythonDistribution},
            standalone_distribution::StandaloneDistribution,
        },
        python_distributions::PYTHON_DISTRIBUTIONS,
    },
    python_packaging::{
        bytecode::{BytecodeCompiler, CompileMode},
        location::{AbstractResourceLocation, ConcreteResourceLocation},
        resource::{BytecodeOptimizationLevel, PythonResource},
        resource_collection::PythonResourceCollector,
    },
    std::{path::Path, sync::Arc},
};

static ENVIRONMENT: Lazy<Environment> =
    Lazy::new(|| Environment::new().expect("error spawning global environment"));

static DISTRIBUTION_CACHE: Lazy<Arc<DistributionCache>> = Lazy::new(|| {
    Arc::new(DistributionCache::new(Some(
        &ENVIRONMENT.python_distributions_dir(),
    )))
});

pub fn get_python_distribution() -> Result<Arc<StandaloneDistribution>> {
    let record = PYTHON_DISTRIBUTIONS
        .find_distribution(
            default_target_triple(),
            &DistributionFlavor::Standalone,
            None,
        )
        .ok_or_else(|| anyhow!("unable to find distribution"))?;

    DISTRIBUTION_CACHE.resolve_distribution(
        &record.location,
        Some(&ENVIRONMENT.cache_dir().join("python_distributions")),
    )
}

pub fn default_interpreter_config<'a>() -> OxidizedPythonInterpreterConfig<'a> {
    let mut config = OxidizedPythonInterpreterConfig::default();
    config.interpreter_config.parse_argv = Some(false);
    config.set_missing_path_configuration = false;
    config.argv = Some(vec!["python".into()]);
    config.interpreter_config.executable = Some("python".into());

    config
}

pub fn get_interpreter_plain<'interpreter, 'resources>(
) -> Result<MainPythonInterpreter<'interpreter, 'resources>> {
    let config = default_interpreter_config();

    let interp = MainPythonInterpreter::new(config)
        .map_err(|e| anyhow!("error creating new interpreter: {}", e.to_string()))?;

    Ok(interp)
}

pub fn get_interpreter_zip<'interpreter, 'resources>(
    zip_path: &Path,
) -> Result<MainPythonInterpreter<'interpreter, 'resources>> {
    // Ideally we'd set up an interpreter with only zip importing. But for
    // maximum compatibility we need to support filesystem import of extension
    // modules.
    let interp = get_interpreter_plain()?;

    interp.with_gil(|py| -> PyResult<()> {
        let sys = py.import("sys")?;
        let sys_path = sys.getattr("path")?;
        sys_path.call_method("insert", (0, zip_path), None)?;

        Ok(())
    })?;

    Ok(interp)
}

pub fn get_interpreter_packed<'interpreter, 'resources>(
    packed_resources: &'resources [u8],
) -> Result<MainPythonInterpreter<'interpreter, 'resources>> {
    let mut config = default_interpreter_config();
    config.oxidized_importer = true;

    config
        .packed_resources
        .push(PackedResourcesSource::Memory(packed_resources));

    let interp = MainPythonInterpreter::new(config)
        .map_err(|e| anyhow!("error creating new interpreter: {}", e.to_string()))?;

    Ok(interp)
}

pub fn get_interpreter_with_oxidized<'interpreter, 'resources>(
) -> Result<MainPythonInterpreter<'interpreter, 'resources>> {
    let mut config = default_interpreter_config();
    // Need this so the extension is importable as a builtin.
    config.oxidized_importer = true;

    MainPythonInterpreter::new(config)
        .map_err(|e| anyhow!("error creating new interpreter: {}", e.to_string()))
}

pub fn get_interpreter_and_oxidized_finder<'interpreter, 'resources>(
    packed_resources: &'resources [u8],
) -> Result<(MainPythonInterpreter<'interpreter, 'resources>, Py<PyAny>)> {
    let interp = get_interpreter_with_oxidized()?;

    let finder = interp.with_gil(|py| -> PyResult<_> {
        let oxidized_importer = py.import("oxidized_importer")?;
        let finder_type = oxidized_importer.getattr("OxidizedFinder")?;
        let finder = finder_type.call0()?;

        let resources_bytes = PyBytes::new(py, packed_resources);
        finder.call_method("index_bytes", (resources_bytes,), None)?;

        let finder = finder.into_py(py);

        Ok(finder)
    })?;

    Ok((interp, finder))
}

pub fn resolve_packed_resources() -> Result<(Vec<u8>, Vec<String>)> {
    let dist = get_python_distribution()?;

    let mut collector = PythonResourceCollector::new(
        vec![AbstractResourceLocation::InMemory],
        vec![AbstractResourceLocation::InMemory],
        false,
        true,
    );

    for resource in dist.python_resources().into_iter() {
        if let PythonResource::ModuleSource(source) = resource {
            if source.name.contains("test") {
                continue;
            }

            collector.add_python_module_source(&source, &ConcreteResourceLocation::InMemory)?;
            collector.add_python_module_bytecode_from_source(
                &source.as_bytecode_module(BytecodeOptimizationLevel::Zero),
                &ConcreteResourceLocation::InMemory,
            )?;
        }
    }

    let temp_dir = tempfile::Builder::new()
        .prefix("pyoxidizer-bench-")
        .tempdir()?;
    let mut compiler = BytecodeCompiler::new(dist.python_exe_path(), temp_dir.path())?;

    let compiled = collector.compile_resources(&mut compiler)?;

    let mut buffer = Vec::<u8>::new();
    compiled.write_packed_resources(&mut buffer)?;

    let names = compiled.resources.keys().cloned().collect::<Vec<_>>();

    Ok((buffer, names))
}

pub fn resolve_zip_archive() -> Result<Vec<u8>> {
    let dist = get_python_distribution()?;

    let temp_dir = tempfile::Builder::new()
        .prefix("pyoxidizer-bench-")
        .tempdir()?;

    let mut compiler = BytecodeCompiler::new(dist.python_exe_path(), temp_dir.path())?;

    for resource in dist.python_resources().into_iter() {
        if let PythonResource::ModuleSource(source) = resource {
            if source.name.contains("test") {
                continue;
            }

            let py_path =
                source.resolve_path(&format!("{}", temp_dir.path().join("lib").display()));
            let pyc_path = py_path.with_extension("pyc");

            let parent = py_path
                .parent()
                .ok_or_else(|| anyhow!("unable to resolve parent path"))?;

            let module_source = source.source.resolve_content()?;

            let bytecode_module = source.as_bytecode_module(BytecodeOptimizationLevel::Zero);
            let bytecode = bytecode_module.compile(&mut compiler, CompileMode::PycUncheckedHash)?;

            std::fs::create_dir_all(parent)?;
            std::fs::write(&py_path, &module_source)?;
            std::fs::write(&pyc_path, &bytecode)?;
        }
    }

    let config = default_interpreter_config();
    let interp = MainPythonInterpreter::new(config)
        .map_err(|e| anyhow!("error creating Python interpreter: {}", e.to_string()))?;

    let archive_path = interp.with_gil(|py| -> PyResult<_> {
        let zipapp = py.import("zipapp")?;

        let archive_path = temp_dir.path().join("stdlib.zip");

        zipapp.call_method(
            "create_archive",
            (
                temp_dir.path().join("lib"),
                &archive_path,
                py.None(),
                "json:tool",
            ),
            None,
        )?;

        Ok(archive_path)
    })?;

    let data = std::fs::read(&archive_path)?;

    Ok(data)
}

pub fn filter_module_names(modules: &[String]) -> Vec<&str> {
    modules
        .iter()
        .filter_map(|x| {
            if !matches!(
                x.as_str(),
                    // Opens a browser.
                    "antigravity"
                    // POSIX only.
                    | "asyncio.unix_events"
                    // Windows only.
                    | "asyncio.windows_events"
                    | "asyncio.windows_utils"
                    // POSIX only.
                    | "crypt"
                    // POSIX only.
                    | "dbm.gnu"
                    | "dbm.ndbm"
                    // Prints output from libmpdec.
                    | "decimal"
                    // Windows only.
                    | "encodings.mbcs"
                    | "encodings.oem"
                    // Prints output from libmpdec.
                    | "fractions"
                    // POSIX only.
                    | "multiprocessing.popen_fork"
                    | "multiprocessing.popen_forkserver"
                    | "multiprocessing.popen_spawn_posix"
                    // Windows only.
                    | "multiprocessing.popen_spawn_win32"
                    // POSIX only.
                    | "pty"
                    // Prints output from libmpdec.
                    | "statistics"
                    | "this"
                    // Build dependent.
                    | "tracemalloc"
                    // POSIX only.
                    | "tty"
                    // Prints output from libmpdec.
                    | "xmlrpc.client"
                    | "xmlrpc.server"
            )
                // Prints output
                && !x.starts_with("__phello__")
                && !x.starts_with("config-")
                && !x.starts_with("ctypes")
                // POSIX only.
                && !x.starts_with("curses")
                // Lots of platform-specific modules.
                && !x.starts_with("distutils")
                // Attempts to execute things.
                && !x.starts_with("ensurepip")
                // Attempts to do GUI things.
                && !x.starts_with("idlelib")
                // Bleh.
                && !x.starts_with("lib2to3")
                // Windows only.
                && !x.starts_with("msilib")
                // Platform specific modules.
                && !x.starts_with("pip")
                // Platform specific modules.
                && !x.starts_with("setuptools")
                // GUI things.
                && !x.starts_with("tkinter")
                && !x.starts_with("turtle")
                // Platform specific modules.
                && !x.starts_with("venv")
            {
                Some(x.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
}
