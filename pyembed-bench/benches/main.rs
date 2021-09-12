// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    criterion::{criterion_group, criterion_main, Criterion},
    pyembed::{MainPythonInterpreter, PackedResourcesSource, PythonResourcesState},
    pyembed_bench::*,
    pyo3::IntoPy,
    python_oxidized_importer::ZipIndex,
    std::io::{BufReader, Cursor, Read, Seek},
    zip::read::ZipArchive,
};

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
