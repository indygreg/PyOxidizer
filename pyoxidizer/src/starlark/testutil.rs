// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        environment::default_target_triple,
        starlark::eval::{EvaluationContext, EvaluationContextBuilder},
        testutil::{get_env, DISTRIBUTION_CACHE},
    },
    anyhow::{anyhow, Result},
    codemap::CodeMap,
    codemap_diagnostic::Diagnostic,
    starlark::values::Value,
};

/// Construct a new `EvaluationContextBuilder` suitable for the test environment.
pub fn test_evaluation_context_builder() -> Result<EvaluationContextBuilder> {
    let env = get_env()?;
    let cwd = std::env::current_dir()?;
    let config_path = cwd.join("dummy");

    let builder = EvaluationContextBuilder::new(&env, config_path, default_target_triple())
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

pub fn starlark_ok(snippet: &str) -> Value {
    let mut eval = test_evaluation_context_builder()
        .unwrap()
        .into_context()
        .unwrap();

    let res = eval.eval(snippet);
    assert!(res.is_ok());

    res.unwrap()
}

pub fn starlark_nok(snippet: &str) -> Diagnostic {
    let mut eval = test_evaluation_context_builder()
        .unwrap()
        .into_context()
        .unwrap();
    let map = std::sync::Arc::new(std::sync::Mutex::new(CodeMap::new()));

    let res = eval.eval_diagnostic(&map, "<nofile>", snippet);

    assert!(res.is_err());

    res.unwrap_err()
}
