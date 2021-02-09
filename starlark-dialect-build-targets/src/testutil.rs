// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{populate_environment, register_starlark_dialect, EnvironmentContext},
    anyhow::{anyhow, Result},
    codemap::CodeMap,
    codemap_diagnostic::{Diagnostic, Emitter},
    slog::Drain,
    starlark::{
        environment::{Environment, TypeValues},
        syntax::dialect::Dialect,
        values::Value,
    },
};

/// A slog Drain that uses println!.
pub struct PrintlnDrain {
    /// Minimum logging level that we're emitting.
    pub min_level: slog::Level,
}

/// slog Drain that uses println!.
impl slog::Drain for PrintlnDrain {
    type Ok = ();
    type Err = std::io::Error;

    fn log(
        &self,
        record: &slog::Record<'_>,
        _values: &slog::OwnedKVList,
    ) -> Result<Self::Ok, Self::Err> {
        if record.level().is_at_least(self.min_level) {
            println!("{}", record.msg());
        }

        Ok(())
    }
}

/// A Starlark execution environment.
///
/// Provides convenience wrappers for common functionality.
pub struct StarlarkEnvironment {
    pub env: Environment,
    pub type_values: TypeValues,
}

impl StarlarkEnvironment {
    pub fn new() -> Result<Self> {
        let logger = slog::Logger::root(
            PrintlnDrain {
                min_level: slog::Level::Info,
            }
            .fuse(),
            slog::o!(),
        );

        let cwd = std::env::current_dir()?;

        let context = EnvironmentContext::new(&logger, cwd);

        let (mut env, mut type_values) = starlark::stdlib::global_environment();
        register_starlark_dialect(&mut env, &mut type_values)
            .map_err(|e| anyhow!("error creating Starlark environment: {:?}", e))?;
        populate_environment(&mut env, &mut type_values, context)
            .map_err(|e| anyhow!("error populating Starlark environment: {:?}", e))?;

        Ok(Self { env, type_values })
    }

    pub fn eval_raw(
        &mut self,
        map: &std::sync::Arc<std::sync::Mutex<CodeMap>>,
        file_loader_env: Environment,
        code: &str,
    ) -> Result<Value, Diagnostic> {
        starlark::eval::simple::eval(
            &map,
            "<test>",
            code,
            Dialect::Bzl,
            &mut self.env,
            &self.type_values,
            file_loader_env,
        )
    }

    /// Evaluate code in the Starlark environment.
    pub fn eval(&mut self, code: &str) -> Result<Value> {
        let map = std::sync::Arc::new(std::sync::Mutex::new(CodeMap::new()));
        let file_loader_env = self.env.clone();

        self.eval_raw(&map, file_loader_env, code)
            .map_err(|diagnostic| {
                let cloned_map_lock = std::sync::Arc::clone(&map);
                let unlocked_map = cloned_map_lock.lock().unwrap();

                let mut buffer = vec![];
                Emitter::vec(&mut buffer, Some(&unlocked_map)).emit(&[diagnostic]);

                anyhow!(
                    "error running '{}': {}",
                    code,
                    String::from_utf8_lossy(&buffer)
                )
            })
    }
}

pub fn starlark_ok(snippet: &str) -> Value {
    let mut env = StarlarkEnvironment::new().expect("error creating starlark environment");

    let res = env.eval(snippet);
    assert!(res.is_ok());

    res.unwrap()
}
