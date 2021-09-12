// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    criterion::{criterion_group, criterion_main, Criterion},
    pyembed::{MainPythonInterpreter, PackedResourcesSource},
    pyembed_bench::*,
};

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

pub fn bench_embedded_interpreter(c: &mut Criterion) {
    let (packed_resources, _) =
        resolve_packed_resources().expect("failed to resolve packed resources");

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

criterion_group!(benches, bench_embedded_interpreter);
criterion_main!(benches);
