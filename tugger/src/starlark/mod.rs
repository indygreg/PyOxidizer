// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
The `starlark` module and related sub-modules define the
[Starlark](https://github.com/bazelbuild/starlark) dialect used by
Tugger.
*/

pub mod apple_universal_binary;
pub mod code_signing;
pub mod file_content;
pub mod file_manifest;
pub mod file_resource;
pub mod macos_application_bundle_builder;
pub mod python_wheel_builder;
pub mod snapcraft;
pub mod terminal;
#[cfg(test)]
mod testutil;
pub mod wix_bundle_builder;
pub mod wix_installer;
pub mod wix_msi_builder;

use {
    console::Term,
    starlark::{
        environment::{Environment, EnvironmentError, TypeValues},
        values::{
            error::{RuntimeError, ValueError},
            Mutable, TypedValue, Value, ValueResult,
        },
    },
    std::ops::{Deref, DerefMut},
};

/// Holds global context for Tugger Starlark evaluation.
pub struct TuggerContext {
    pub term_stdout: Term,
    pub term_stderr: Term,
    pub code_signers: Vec<Value>,
    /// Whether to forcefully disable user interaction.
    ///
    /// Setting to true causes [Self::can_prompt] to always return false.
    pub disable_interaction: bool,
}

impl TuggerContext {
    pub fn new() -> Self {
        Self {
            // Hard-coded to stdout for now. We'll probably want to make this configurable
            // to facilitate testing.
            term_stdout: Term::stdout(),
            term_stderr: Term::stderr(),
            code_signers: vec![],
            disable_interaction: false,
        }
    }

    /// Whether we can prompt for input.
    pub fn can_prompt(&self) -> bool {
        !self.disable_interaction && atty::is(atty::Stream::Stdin)
    }
}

pub struct TuggerContextValue {
    inner: TuggerContext,
}

impl Deref for TuggerContextValue {
    type Target = TuggerContext;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for TuggerContextValue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl TypedValue for TuggerContextValue {
    type Holder = Mutable<TuggerContextValue>;
    const TYPE: &'static str = "TuggerContext";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(self.code_signers.clone().into_iter())
    }
}

#[derive(Default)]
pub struct TuggerContextHolder {}

impl TypedValue for TuggerContextHolder {
    type Holder = Mutable<TuggerContextHolder>;
    const TYPE: &'static str = "Tugger";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

const ENVIRONMENT_CONTEXT_SYMBOL: &str = "TUGGER_CONTEXT";

pub fn get_context_value(type_values: &TypeValues) -> ValueResult {
    type_values
        .get_type_value(
            &Value::new(TuggerContextHolder::default()),
            ENVIRONMENT_CONTEXT_SYMBOL,
        )
        .ok_or_else(|| {
            ValueError::from(RuntimeError {
                code: "TUGGER",
                message: "unable to resolve context (this should never happen)".to_string(),
                label: "".to_string(),
            })
        })
}

/// Registers Tugger's Starlark dialect.
pub fn register_starlark_dialect(
    env: &mut Environment,
    type_values: &mut TypeValues,
) -> Result<(), EnvironmentError> {
    apple_universal_binary::apple_universal_binary_module(env, type_values);
    code_signing::code_signing_module(env, type_values);
    file_content::file_content_module(env, type_values);
    file_manifest::file_manifest_module(env, type_values);
    file_resource::file_resource_module(env, type_values);
    macos_application_bundle_builder::macos_application_bundle_builder_module(env, type_values);
    python_wheel_builder::python_wheel_builder_module(env, type_values);
    snapcraft::snapcraft_module(env, type_values);
    terminal::terminal_module(env, type_values);
    wix_bundle_builder::wix_bundle_builder_module(env, type_values);
    wix_installer::wix_installer_module(env, type_values);
    wix_msi_builder::wix_msi_builder_module(env, type_values);

    Ok(())
}

/// Populate a Starlark environment with variables needed to support this dialect.
pub fn populate_environment(
    env: &mut Environment,
    type_values: &mut TypeValues,
    context: TuggerContext,
) -> Result<(), EnvironmentError> {
    env.set(
        ENVIRONMENT_CONTEXT_SYMBOL,
        Value::new(TuggerContextValue { inner: context }),
    )?;

    let symbol = &ENVIRONMENT_CONTEXT_SYMBOL;
    type_values.add_type_value(TuggerContextHolder::TYPE, symbol, env.get(symbol)?);

    Ok(())
}

#[cfg(test)]
mod tests {
    use {super::*, crate::starlark::testutil::*, anyhow::Result};

    #[test]
    fn test_get_context() -> Result<()> {
        let env = StarlarkEnvironment::new()?;

        let context_value = get_context_value(&env.type_values).unwrap();
        context_value.downcast_ref::<TuggerContextValue>().unwrap();

        Ok(())
    }
}
