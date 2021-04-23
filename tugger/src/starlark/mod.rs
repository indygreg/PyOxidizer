// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
The `starlark` module and related sub-modules define the
[Starlark](https://github.com/bazelbuild/starlark) dialect used by
Tugger.
*/

pub mod file_resource;
pub mod macos_application_bundle_builder;
pub mod snapcraft;
#[cfg(test)]
mod testutil;
pub mod wix_bundle_builder;
pub mod wix_installer;
pub mod wix_msi_builder;

use {
    starlark::{
        environment::{Environment, EnvironmentError, TypeValues},
        values::{
            error::{RuntimeError, ValueError},
            Immutable, Mutable, TypedValue, Value, ValueResult,
        },
    },
    std::ops::Deref,
};

/// Holds global context for Tugger Starlark evaluation.
#[derive(Default)]
pub struct TuggerContext {}

#[derive(Default)]
pub struct TuggerContextValue {
    inner: TuggerContext,
}

impl Deref for TuggerContextValue {
    type Target = TuggerContext;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl TypedValue for TuggerContextValue {
    type Holder = Immutable<TuggerContextValue>;
    const TYPE: &'static str = "TuggerContext";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
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
    file_resource::file_resource_module(env, type_values);
    macos_application_bundle_builder::macos_application_bundle_builder_module(env, type_values);
    snapcraft::snapcraft_module(env, type_values);
    wix_bundle_builder::wix_bundle_builder_module(env, type_values);
    wix_installer::wix_installer_module(env, type_values);
    wix_msi_builder::wix_msi_builder_module(env, type_values);

    Ok(())
}

/// Populate a Starlark environment with variables needed to support this dialect.
pub fn populate_environment(
    env: &mut Environment,
    type_values: &mut TypeValues,
) -> Result<(), EnvironmentError> {
    env.set(
        ENVIRONMENT_CONTEXT_SYMBOL,
        Value::new(TuggerContextValue::default()),
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
