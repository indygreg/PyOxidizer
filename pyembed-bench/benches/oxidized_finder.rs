// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    criterion::{criterion_group, criterion_main, Criterion},
    pyembed::{MainPythonInterpreter, PythonResourcesState},
    pyembed_bench::*,
    python_packaging::resource::BytecodeOptimizationLevel,
};

fn parse_packed_resources(data: &[u8]) -> Result<()> {
    let resources = python_packed_resources::load_resources(data)
        .map_err(|e| anyhow!("failed loaded packed resources data: {}", e))?;
    for r in resources {
        r.map_err(|e| anyhow!("resource error: {}", e))?;
    }

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

fn python_resources_state_resolve_modules(
    state: &PythonResourcesState<u8>,
    modules: &[String],
) -> Result<()> {
    for name in modules {
        state
            .resolve_importable_module(name, BytecodeOptimizationLevel::Zero)
            .expect("failed to retrieve module");
    }

    Ok(())
}

fn python_interpreter_import_all_modules(
    interp: &MainPythonInterpreter,
    modules: &[&str],
) -> Result<()> {
    interp.with_gil(|py| -> Result<_> {
        for name in modules {
            // println!("{}", name);
            py.import(*name).map_err(|e| {
                e.print(py);
                anyhow!("error importing module {}", name)
            })?;
        }

        Ok(())
    })?;

    Ok(())
}

pub fn bench_oxidized_finder(c: &mut Criterion) {
    let (packed_resources, names) =
        resolve_packed_resources().expect("failed to resolve packed resources");
    let importable_modules = filter_module_names(&names);
    println!(
        "{} bytes packed resources data for {} modules; {} importable",
        packed_resources.len(),
        names.len(),
        importable_modules.len()
    );

    let mut resources_state =
        PythonResourcesState::new_from_env().expect("failed to create resources state");
    resources_state
        .index_data(&packed_resources)
        .expect("failed to index resources data");

    c.bench_function("python-packed-resources.parse", |b| {
        b.iter(|| {
            parse_packed_resources(&packed_resources).expect("failed to parse packed resources")
        })
    });

    c.bench_function("oxidized_importer.PythonResourcesState.index_data", |b| {
        b.iter(|| python_resources_state_index(&packed_resources).expect("failed to index data"))
    });

    c.bench_function(
        "oxidized_importer.PythonResourcesState.resolve_modules",
        |b| {
            b.iter(|| {
                python_resources_state_resolve_modules(&resources_state, &names)
                    .expect("failed to resolve modules")
            })
        },
    );

    c.bench_function(
        "oxidized_importer.OxidizedFinder.in_memory.find_spec.all_modules",
        |b| {
            b.iter_with_setup(
                || {
                    get_interpreter_and_oxidized_finder(&packed_resources)
                        .expect("failed to obtain OxidizedFinder")
                },
                |(interp, finder)| {
                    interp.with_gil(|py| {
                        let finder = finder.as_ref(py);

                        for name in &names {
                            finder
                                .call_method("find_spec", (name, py.None()), None)
                                .expect("find_spec call failed");
                        }
                    });
                    std::mem::drop(interp);
                },
            )
        },
    );

    c.bench_function(
        "oxidized_importer.OxidizedFinder.in_memory.get_source.all_modules",
        |b| {
            b.iter_with_setup(
                || {
                    get_interpreter_and_oxidized_finder(&packed_resources)
                        .expect("failed to obtain OxidizedFinder")
                },
                |(interp, finder)| {
                    interp.with_gil(|py| {
                        let finder = finder.as_ref(py);

                        for name in &names {
                            finder
                                .call_method("get_source", (name,), None)
                                .expect("get_source call failed");
                        }
                    });
                    std::mem::drop(interp);
                },
            )
        },
    );

    c.bench_function(
        "oxidized_importer.OxidizedFinder.in_memory.get_code.all_modules",
        |b| {
            b.iter_with_setup(
                || {
                    get_interpreter_and_oxidized_finder(&packed_resources)
                        .expect("failed to obtain OxidizedFinder")
                },
                |(interp, finder)| {
                    interp.with_gil(|py| {
                        let finder = finder.as_ref(py);

                        for name in &names {
                            finder
                                .call_method("get_code", (name,), None)
                                .expect("get_code call failed");
                        }
                    });
                    std::mem::drop(interp);
                },
            )
        },
    );

    c.bench_function("oxidized_importer.PathFinder.import_all_modules", |b| {
        b.iter_with_setup(
            || get_interpreter_plain().expect("unable to obtain interpreter"),
            |interp| {
                python_interpreter_import_all_modules(&interp, &importable_modules)
                    .expect("failed to import all modules");
                std::mem::drop(interp);
            },
        )
    });

    c.bench_function(
        "oxidized_importer.OxidizedFinder.in_memory.import_all_modules",
        |b| {
            b.iter_with_setup(
                || get_interpreter_packed(&packed_resources).expect("unable to obtain interpreter"),
                |interp| {
                    python_interpreter_import_all_modules(&interp, &importable_modules)
                        .expect("failed to import all modules");
                    std::mem::drop(interp);
                },
            )
        },
    );
}

criterion_group!(benches, bench_oxidized_finder);
criterion_main!(benches);
