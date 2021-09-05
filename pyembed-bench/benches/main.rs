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
    pyoxidizerlib::{
        environment::{default_target_triple, Environment},
        logging::PrintlnDrain,
        py_packaging::distribution::{DistributionCache, DistributionFlavor, PythonDistribution},
        python_distributions::PYTHON_DISTRIBUTIONS,
    },
    python_packaging::{
        bytecode::BytecodeCompiler,
        location::{AbstractResourceLocation, ConcreteResourceLocation},
        resource::{BytecodeOptimizationLevel, PythonResource},
        resource_collection::PythonResourceCollector,
    },
    slog::{Drain, Logger},
    std::sync::Arc,
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

fn default_interpreter_config<'a>() -> OxidizedPythonInterpreterConfig<'a> {
    let mut config = OxidizedPythonInterpreterConfig::default();
    config.interpreter_config.parse_argv = Some(false);
    config.set_missing_path_configuration = false;
    config.argv = Some(vec!["python".into()]);
    config.interpreter_config.executable = Some("python".into());

    config
}

fn resolve_packed_resources() -> Result<Vec<u8>> {
    let logger = get_logger()?;

    let record = PYTHON_DISTRIBUTIONS
        .find_distribution(
            default_target_triple(),
            &DistributionFlavor::Standalone,
            None,
        )
        .ok_or_else(|| anyhow!("unable to find distribution"))?;

    let dist = DISTRIBUTION_CACHE.resolve_distribution(
        &logger,
        &record.location,
        Some(&ENVIRONMENT.cache_dir().join("python_distributions")),
    )?;

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

    Ok(buffer)
}

fn parse_packed_resources(data: &[u8]) -> Result<()> {
    let resources = python_packed_resources::parser::load_resources(data)
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

pub fn criterion_benchmark(c: &mut Criterion) {
    let packed_resources = resolve_packed_resources().expect("failed to resolve packed resources");

    c.bench_function("python-packed-resources.parse", |b| {
        b.iter(|| {
            parse_packed_resources(&packed_resources).expect("failed to parse packed resources")
        })
    });

    c.bench_function("PythonResourcesState.index_data", |b| {
        b.iter(|| python_resources_state_index(&packed_resources).expect("failed to index data"))
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
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
