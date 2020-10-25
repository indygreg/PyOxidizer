// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::env::{get_context, global_environment, PyOxidizerEnvironmentContext},
    crate::{logging::PrintlnDrain, testutil::DISTRIBUTION_CACHE},
    anyhow::{anyhow, Result},
    codemap::CodeMap,
    codemap_diagnostic::{Diagnostic, Emitter},
    slog::Drain,
    starlark::{
        environment::{Environment, TypeValues},
        eval,
        syntax::dialect::Dialect,
        values::Value,
    },
};

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

        let build_target = crate::project_building::HOST;

        let cwd = std::env::current_dir()?;
        let config_path = cwd.join("dummy");

        let context = PyOxidizerEnvironmentContext::new(
            &logger,
            false,
            &config_path,
            build_target,
            build_target,
            false,
            "0",
            Some(DISTRIBUTION_CACHE.clone()),
        )?;

        let (env, type_values) = global_environment(context, None, false)
            .map_err(|e| anyhow!("error creating Starlark environment: {:?}", e))?;

        Ok(Self { env, type_values })
    }

    /// Create a new environment with `dist` and `exe` variables set.
    pub fn new_with_exe() -> Result<Self> {
        let mut env = Self::new()?;

        env.eval("dist = default_python_distribution()")?;
        env.eval("exe = dist.to_python_executable('testapp')")?;

        Ok(env)
    }

    pub fn eval_raw(
        &mut self,
        map: &std::sync::Arc<std::sync::Mutex<CodeMap>>,
        file_loader_env: Environment,
        code: &str,
    ) -> Result<Value, Diagnostic> {
        eval::simple::eval(
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

    pub fn eval_assert(&mut self, code: &str) -> Result<()> {
        let value = self.eval(code)?;

        if value.get_type() != "bool" || !value.to_bool() {
            Err(anyhow!("{} does not evaluate to True", code))
        } else {
            Ok(())
        }
    }

    pub fn get(&self, name: &str) -> Result<Value> {
        let value = self.env.get(name).unwrap();

        Ok(value)
    }

    pub fn set(&mut self, name: &str, value: Value) -> Result<()> {
        self.env.set(name, value).unwrap();

        Ok(())
    }

    /// Set the target triple we are building for.
    ///
    /// This needs to be called shortly after construction or things won't work
    /// as expected.
    pub fn set_target_triple(&mut self, triple: &str) -> Result<()> {
        let pyoxidizer_context_value = get_context(&self.type_values).unwrap();
        let mut pyoxidizer_context = pyoxidizer_context_value
            .downcast_mut::<PyOxidizerEnvironmentContext>()
            .unwrap()
            .unwrap();

        pyoxidizer_context.build_target_triple = triple.to_string();

        Ok(())
    }
}

pub fn starlark_ok(snippet: &str) -> Value {
    let mut env = StarlarkEnvironment::new().expect("error creating starlark environment");

    let res = env.eval(snippet);
    assert!(res.is_ok());

    res.unwrap()
}

pub fn starlark_nok(snippet: &str) -> Diagnostic {
    let mut env = StarlarkEnvironment::new().expect("error creating starlark environment");
    let map = std::sync::Arc::new(std::sync::Mutex::new(CodeMap::new()));
    let file_loader_env = env.env.clone();

    let res = env.eval_raw(&map, file_loader_env, snippet);

    assert!(res.is_err());

    res.unwrap_err()
}
