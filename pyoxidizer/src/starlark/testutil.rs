// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        logging::PrintlnDrain,
        starlark::eval::{EvaluationContext, EvaluationContextBuilder},
        testutil::DISTRIBUTION_CACHE,
    },
    anyhow::{anyhow, Result},
    codemap::CodeMap,
    codemap_diagnostic::Diagnostic,
    slog::Drain,
    starlark::values::Value,
};

/// Construct a new `EvaluationContextBuilder` suitable for the test environment.
pub fn test_evaluation_context_builder() -> Result<EvaluationContextBuilder> {
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

    let builder = EvaluationContextBuilder::new(logger, config_path, build_target)
        .distribution_cache(DISTRIBUTION_CACHE.clone());

    Ok(builder)
}

/// Add a PythonExecutable `exe` variable to the Starlark environment.
pub fn add_exe(eval: &mut EvaluationContext) -> Result<()> {
    eval.eval("dist = default_python_distribution()")?;
    eval.eval("exe = dist.to_python_executable('testapp')")?;

    Ok(())
}

pub fn eval_assert(eval: &mut EvaluationContext, code: &str) -> Result<()> {
    let value = eval.eval(code)?;

    if value.get_type() != "bool" || !value.to_bool() {
        Err(anyhow!("{} does not evaluate to True", code))
    } else {
        Ok(())
    }
}

/// A Starlark execution environment.
///
/// Provides convenience wrappers for common functionality.
pub struct StarlarkEnvironment {
    pub eval: EvaluationContext,
}

impl StarlarkEnvironment {
    pub fn new() -> Result<Self> {
        let eval = test_evaluation_context_builder()?.into_context()?;

        Ok(Self { eval })
    }

    pub fn eval_raw(
        &mut self,
        map: &std::sync::Arc<std::sync::Mutex<CodeMap>>,
        code: &str,
    ) -> Result<Value, Diagnostic> {
        self.eval.eval_diagnostic(&map, "<test>", code)
    }

    /// Evaluate code in the Starlark environment.
    pub fn eval(&mut self, code: &str) -> Result<Value> {
        self.eval.eval_code_with_path("<test>", code)
    }

    pub fn get(&self, name: &str) -> Result<Value> {
        let value = self.eval.get_var(name).unwrap();

        Ok(value)
    }

    pub fn set(&mut self, name: &str, value: Value) -> Result<()> {
        self.eval.set_var(name, value).unwrap();

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

    let res = env.eval_raw(&map, snippet);

    assert!(res.is_err());

    res.unwrap_err()
}
