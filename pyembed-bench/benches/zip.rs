// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    criterion::{criterion_group, criterion_main, Criterion},
    oxidized_importer::ZipIndex,
    pyembed::MainPythonInterpreter,
    pyembed_bench::*,
    pyo3::IntoPy,
    std::io::{BufReader, Cursor, Read, Seek},
    zip::read::ZipArchive,
};

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

fn python_interpreter_import_all_modules(
    interp: &MainPythonInterpreter,
    modules: &[&str],
) -> Result<()> {
    interp.with_gil(|py| {
        for name in modules {
            // println!("{}", name);
            py.import(*name).map_err(|e| {
                e.print(py);
                anyhow!("error importing module {}", name)
            })?;
        }

        Ok(())
    })
}

pub fn bench_zip(c: &mut Criterion) {
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

    c.bench_function("oxidized_importer.zipimport.import_all_modules", |b| {
        b.iter_with_setup(
            || get_interpreter_zip(&zip_path).expect("unable to obtain interpreter"),
            |interp| {
                python_interpreter_import_all_modules(&interp, &importable_modules)
                    .expect("failed to import all modules");
                std::mem::drop(interp);
            },
        )
    });

    c.bench_function(
        "oxidized_importer.OxidizedZipFinder.from_data.import_all_modules",
        |b| {
            b.iter_with_setup(
                || {
                    let interp =
                        get_interpreter_with_oxidized().expect("unable to obtain interpreter");
                    let zip_data_bytes = interp.with_gil(|py| zip_data.as_slice().into_py(py));

                    (interp, zip_data_bytes)
                },
                |(interp, zip_data_bytes)| {
                    interp.with_gil(|py| {
                        let oxidized_importer = py.import("oxidized_importer").unwrap();
                        let zip_type = oxidized_importer.getattr("OxidizedZipFinder").unwrap();
                        let constructor = zip_type.getattr("from_zip_data").unwrap();
                        let finder = constructor.call((zip_data_bytes,), None).unwrap();
                        let sys = py.import("sys").unwrap();
                        let meta_path = sys.getattr("meta_path").unwrap();
                        meta_path.call_method("insert", (0, finder), None).unwrap();
                    });

                    python_interpreter_import_all_modules(&interp, &importable_modules)
                        .expect("failed to import all modules");
                    std::mem::drop(interp);
                },
            )
        },
    );

    c.bench_function(
        "oxidized_importer.OxidizedZipFinder.from_path.import_all_modules",
        |b| {
            b.iter_with_setup(
                || get_interpreter_with_oxidized().expect("unable to obtain interpreter"),
                |interp| {
                    interp.with_gil(|py| {
                        let oxidized_importer = py.import("oxidized_importer").unwrap();
                        let zip_type = oxidized_importer.getattr("OxidizedZipFinder").unwrap();
                        let constructor = zip_type.getattr("from_path").unwrap();
                        let finder = constructor
                            .call((format!("{}", zip_path.display()),), None)
                            .unwrap();
                        let sys = py.import("sys").unwrap();
                        let meta_path = sys.getattr("meta_path").unwrap();
                        meta_path.call_method("insert", (0, finder), None).unwrap();
                    });

                    python_interpreter_import_all_modules(&interp, &importable_modules)
                        .expect("failed to import all modules");
                    std::mem::drop(interp);
                },
            )
        },
    );
}

criterion_group!(benches, bench_zip);
criterion_main!(benches);
