// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    criterion::{criterion_group, criterion_main, Criterion},
    once_cell::sync::Lazy,
    pyembed::{
        MainPythonInterpreter, OxidizedPythonInterpreterConfig, PackedResourcesSource,
        PythonResourcesState,
    },
    pyo3::IntoPy,
    pyoxidizerlib::{
        environment::{default_target_triple, Environment},
        logging::PrintlnDrain,
        py_packaging::{
            distribution::{DistributionCache, DistributionFlavor, PythonDistribution},
            standalone_distribution::StandaloneDistribution,
        },
        python_distributions::PYTHON_DISTRIBUTIONS,
    },
    python_oxidized_importer::ZipIndex,
    python_packaging::{
        bytecode::{BytecodeCompiler, CompileMode},
        location::{AbstractResourceLocation, ConcreteResourceLocation},
        resource::{BytecodeOptimizationLevel, PythonResource},
        resource_collection::PythonResourceCollector,
    },
    slog::{Drain, Logger},
    std::{
        io::{BufReader, Cursor, Read, Seek},
        path::Path,
        sync::Arc,
    },
    zip::read::ZipArchive,
};

static ENVIRONMENT: Lazy<Environment> =
    Lazy::new(|| Environment::new().expect("error spawning global environment"));

static DISTRIBUTION_CACHE: Lazy<Arc<DistributionCache>> = Lazy::new(|| {
    Arc::new(DistributionCache::new(Some(
        &ENVIRONMENT.python_distributions_dir(),
    )))
});

fn get_logger() -> Result<Logger> {
    Ok(Logger::root(
        PrintlnDrain {
            min_level: slog::Level::Warning,
        }
        .fuse(),
        slog::o!(),
    ))
}

fn get_python_distribution() -> Result<Arc<StandaloneDistribution>> {
    let logger = get_logger()?;

    let record = PYTHON_DISTRIBUTIONS
        .find_distribution(
            default_target_triple(),
            &DistributionFlavor::Standalone,
            None,
        )
        .ok_or_else(|| anyhow!("unable to find distribution"))?;

    DISTRIBUTION_CACHE.resolve_distribution(
        &logger,
        &record.location,
        Some(&ENVIRONMENT.cache_dir().join("python_distributions")),
    )
}

fn default_interpreter_config<'a>() -> OxidizedPythonInterpreterConfig<'a> {
    let mut config = OxidizedPythonInterpreterConfig::default();
    config.interpreter_config.parse_argv = Some(false);
    config.set_missing_path_configuration = false;
    config.argv = Some(vec!["python".into()]);
    config.interpreter_config.executable = Some("python".into());

    config
}

fn get_interpreter_plain<'interpreter, 'resources>(
) -> Result<MainPythonInterpreter<'interpreter, 'resources>> {
    let config = default_interpreter_config();

    let interp = MainPythonInterpreter::new(config)
        .map_err(|e| anyhow!("error creating new interpreter: {}", e.to_string()))?;

    Ok(interp)
}

fn get_interpreter_zip<'interpreter, 'resources>(
    zip_path: &Path,
) -> Result<MainPythonInterpreter<'interpreter, 'resources>> {
    // Ideally we'd set up an interpreter with only zip importing. But for
    // maximum compatibility we need to support filesystem import of extension
    // modules.
    let mut interp = get_interpreter_plain()?;

    let py = interp.acquire_gil();
    let sys = py.import("sys")?;
    let sys_path = sys.getattr("path")?;
    sys_path.call_method("insert", (0, zip_path), None)?;

    Ok(interp)
}

fn get_interpreter_packed<'interpreter, 'resources>(
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

fn get_interpreter_with_oxidized<'interpreter, 'resources>(
) -> Result<MainPythonInterpreter<'interpreter, 'resources>> {
    let mut config = default_interpreter_config();
    // Need this so the extension is importable as a builtin.
    config.oxidized_importer = true;

    MainPythonInterpreter::new(config)
        .map_err(|e| anyhow!("error creating new interpreter: {}", e.to_string()))
}

fn resolve_packed_resources() -> Result<(Vec<u8>, Vec<String>)> {
    let dist = get_python_distribution()?;

    let mut collector = PythonResourceCollector::new(
        vec![AbstractResourceLocation::InMemory],
        vec![AbstractResourceLocation::InMemory],
        false,
        true,
        dist.cache_tag(),
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

fn resolve_zip_archive() -> Result<Vec<u8>> {
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

            std::fs::create_dir_all(&parent)?;
            std::fs::write(&py_path, &module_source)?;
            std::fs::write(&pyc_path, &bytecode)?;
        }
    }

    let config = default_interpreter_config();
    let mut interp = MainPythonInterpreter::new(config)
        .map_err(|e| anyhow!("error creating Python interpreter: {}", e.to_string()))?;
    let py = interp.acquire_gil();

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

    let data = std::fs::read(&archive_path)?;

    Ok(data)
}

fn filter_module_names(modules: &[String]) -> Vec<&str> {
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

fn parse_packed_resources(data: &[u8]) -> Result<()> {
    let resources = python_packed_resources::parser::load_resources(data)
        .map_err(|e| anyhow!("failed loaded packed resources data: {}", e))?;
    for r in resources {
        r.map_err(|e| anyhow!("resource error: {}", e))?;
    }

    Ok(())
}

fn zip_parse_rust(source: impl Read + Seek, read: bool) -> Result<()> {
    let mut za = ZipArchive::new(source)?;

    for idx in 0..za.len() {
        let mut zf = za.by_index(idx)?;

        if read {
            let mut buffer = Vec::<u8>::with_capacity(zf.size() as _);
            zf.read_to_end(&mut buffer)?;
        }
    }

    Ok(())
}

fn zip_index(source: impl Read + Seek) -> Result<()> {
    ZipIndex::new(source, None)?;

    Ok(())
}

fn python_resources_state_index(data: &[u8]) -> Result<()> {
    let mut state = PythonResourcesState::new_from_env()
        .map_err(|e| anyhow!("error obtaining PythonResourcesState: {}", e))?;

    state
        .index_data(data)
        .map_err(|e| anyhow!("error indexing data: {}", e))?;

    Ok(())
}

fn python_interpreter_startup_teardown_plain() -> Result<()> {
    let mut config = default_interpreter_config();
    config.interpreter_config.run_command = Some("i = 42".to_string());

    let interp = MainPythonInterpreter::new(config)
        .map_err(|e| anyhow!("error creating new interpreter: {}", e.to_string()))?;
    interp.run();

    Ok(())
}

fn python_interpreter_startup_teardown_packed_resources(packed_resources: &[u8]) -> Result<()> {
    let mut config = default_interpreter_config();
    config.oxidized_importer = true;

    config
        .packed_resources
        .push(PackedResourcesSource::Memory(packed_resources));
    config.interpreter_config.run_command = Some("i = 42".to_string());

    let interp = MainPythonInterpreter::new(config)
        .map_err(|e| anyhow!("error creating new interpreter: {}", e.to_string()))?;
    interp.run();

    Ok(())
}

fn python_interpreter_import_all_modules(
    interp: &mut MainPythonInterpreter,
    modules: &[&str],
) -> Result<()> {
    let py = interp.acquire_gil();

    for name in modules {
        // println!("{}", name);
        py.import(name).map_err(|e| {
            e.print(py);
            anyhow!("error importing module {}", name)
        })?;
    }

    Ok(())
}

pub fn criterion_benchmark(c: &mut Criterion) {
    let temp_dir = tempfile::Builder::new()
        .prefix("pyoxidizer-bench-")
        .tempdir()
        .expect("failed to create temp directory");

    let (packed_resources, names) =
        resolve_packed_resources().expect("failed to resolve packed resources");
    let importable_modules = filter_module_names(&names);
    println!(
        "{} bytes packed resources data for {} modules; {} importable",
        packed_resources.len(),
        names.len(),
        importable_modules.len()
    );

    let zip_data = resolve_zip_archive().expect("failed to resolve zip archive");
    println!("zip archive {} bytes", zip_data.len());
    let zip_path = temp_dir.path().join("stdlib.zip");
    std::fs::write(&zip_path, &zip_data).expect("failed to write zip archive");

    c.bench_function("python-packed-resources.parse", |b| {
        b.iter(|| {
            parse_packed_resources(&packed_resources).expect("failed to parse packed resources")
        })
    });

    c.bench_function("PythonResourcesState.index_data", |b| {
        b.iter(|| python_resources_state_index(&packed_resources).expect("failed to index data"))
    });

    c.bench_function("zip.parse_rust.memory.no_read", |b| {
        b.iter(|| {
            zip_parse_rust(Cursor::new(&zip_data), false).expect("failed to parse zip archive")
        })
    });

    c.bench_function("zip.parse_rust.memory.read", |b| {
        b.iter(|| {
            zip_parse_rust(Cursor::new(&zip_data), true).expect("failed to parse zip archive")
        })
    });

    c.bench_function("zip.parse_rust.file_unbuffered.no_read", |b| {
        b.iter(|| {
            zip_parse_rust(
                std::fs::File::open(&zip_path).expect("failed to open zip file"),
                false,
            )
            .expect("failed to parse zip archive")
        })
    });

    c.bench_function("zip.parse_rust.file_unbuffered.read", |b| {
        b.iter(|| {
            zip_parse_rust(
                std::fs::File::open(&zip_path).expect("failed to open zip file"),
                true,
            )
            .expect("failed to parse zip archive")
        })
    });

    c.bench_function("zip.parse_rust.file_buffered.no_read", |b| {
        b.iter(|| {
            zip_parse_rust(
                BufReader::new(std::fs::File::open(&zip_path).expect("failed to open zip file")),
                false,
            )
            .expect("failed to parse zip archive")
        })
    });

    c.bench_function("zip.parse_rust.file_buffered.read", |b| {
        b.iter(|| {
            zip_parse_rust(
                BufReader::new(std::fs::File::open(&zip_path).expect("failed to open zip file")),
                true,
            )
            .expect("failed to parse zip archive")
        })
    });

    c.bench_function("oxidized_importer.ZipIndex.new.memory", |b| {
        b.iter(|| {
            zip_index(Cursor::new(&zip_data)).expect("failed to create ZipIndex");
        })
    });

    c.bench_function("oxidized_importer.ZipIndex.new.file_unbuferred", |b| {
        b.iter(|| {
            zip_index(std::fs::File::open(&zip_path).expect("failed to open zip file"))
                .expect("failed to create ZipIndex")
        })
    });

    c.bench_function("oxidized_importer.ZipIndex.new.file_buferred", |b| {
        b.iter(|| {
            zip_index(BufReader::new(
                std::fs::File::open(&zip_path).expect("failed to open zip file"),
            ))
            .expect("failed to create ZipIndex")
        })
    });

    c.bench_function("pyembed.new_interpreter_plain", |b| {
        b.iter(|| python_interpreter_startup_teardown_plain().expect("Python interpreter run"))
    });

    c.bench_function("pyembed.new_interpreter_packed_resources", |b| {
        b.iter(|| {
            python_interpreter_startup_teardown_packed_resources(&packed_resources)
                .expect("Python interpreter run")
        })
    });

    c.bench_function("oxidized_importer.import_all_modules.filesystem", |b| {
        b.iter_with_setup(
            || get_interpreter_plain().expect("unable to obtain interpreter"),
            |mut interp| {
                python_interpreter_import_all_modules(&mut interp, &importable_modules)
                    .expect("failed to import all modules");
                std::mem::drop(interp);
            },
        )
    });

    c.bench_function("oxidized_importer.import_all_modules.zipimport", |b| {
        b.iter_with_setup(
            || get_interpreter_zip(&zip_path).expect("unable to obtain interpreter"),
            |mut interp| {
                python_interpreter_import_all_modules(&mut interp, &importable_modules)
                    .expect("failed to import all modules");
                std::mem::drop(interp);
            },
        )
    });

    c.bench_function(
        "oxidized_importer.import_all_modules.OxidizedFinder.in_memory",
        |b| {
            b.iter_with_setup(
                || get_interpreter_packed(&packed_resources).expect("unable to obtain interpreter"),
                |mut interp| {
                    python_interpreter_import_all_modules(&mut interp, &importable_modules)
                        .expect("failed to import all modules");
                    std::mem::drop(interp);
                },
            )
        },
    );

    c.bench_function(
        "oxidized_importer.import_all_modules.OxidizedZipFinder.from_data",
        |b| {
            b.iter_with_setup(
                || {
                    let mut interp =
                        get_interpreter_with_oxidized().expect("unable to obtain interpreter");
                    let py = interp.acquire_gil();
                    let zip_data_bytes = zip_data.as_slice().into_py(py);

                    (interp, zip_data_bytes)
                },
                |(mut interp, zip_data_bytes)| {
                    {
                        let py = interp.acquire_gil();
                        let oxidized_importer = py.import("oxidized_importer").unwrap();
                        let zip_type = oxidized_importer.getattr("OxidizedZipFinder").unwrap();
                        let constructor = zip_type.getattr("from_zip_data").unwrap();
                        let finder = constructor.call((zip_data_bytes,), None).unwrap();
                        let sys = py.import("sys").unwrap();
                        let meta_path = sys.getattr("meta_path").unwrap();
                        meta_path.call_method("insert", (0, finder), None).unwrap();
                    }

                    python_interpreter_import_all_modules(&mut interp, &importable_modules)
                        .expect("failed to import all modules");
                    std::mem::drop(interp);
                },
            )
        },
    );

    c.bench_function(
        "oxidized_importer.import_all_modules.OxidizedZipFinder.from_path",
        |b| {
            b.iter_with_setup(
                || get_interpreter_with_oxidized().expect("unable to obtain interpreter"),
                |mut interp| {
                    {
                        let py = interp.acquire_gil();
                        let oxidized_importer = py.import("oxidized_importer").unwrap();
                        let zip_type = oxidized_importer.getattr("OxidizedZipFinder").unwrap();
                        let constructor = zip_type.getattr("from_path").unwrap();
                        let finder = constructor
                            .call((format!("{}", zip_path.display()),), None)
                            .unwrap();
                        let sys = py.import("sys").unwrap();
                        let meta_path = sys.getattr("meta_path").unwrap();
                        meta_path.call_method("insert", (0, finder), None).unwrap();
                    }

                    python_interpreter_import_all_modules(&mut interp, &importable_modules)
                        .expect("failed to import all modules");
                    std::mem::drop(interp);
                },
            )
        },
    );
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
