// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::{get_context_value, TuggerContextValue},
    anyhow::anyhow,
    console::Term,
    dialoguer::{Confirm, Input, Password},
    starlark::{
        environment::TypeValues,
        values::{
            error::{RuntimeError, ValueError},
            none::NoneType,
            {Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    starlark_dialect_build_targets::{optional_bool_arg, optional_str_arg},
};

fn error_context<F, T>(label: &str, f: F) -> Result<T, ValueError>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    f().map_err(|e| {
        ValueError::Runtime(RuntimeError {
            code: "TUGGER_TERMINAL",
            message: format!("{:?}", e),
            label: label.to_string(),
        })
    })
}

fn prompt_or_default<T>(term: &Term, default: Option<T>) -> anyhow::Result<T> {
    if let Some(default) = default {
        term.write_line("cannot prompt; using provided default value")?;
        Ok(default)
    } else {
        Err(anyhow!("cannot prompt; is stdin connected to a TTY?"))
    }
}

fn starlark_can_prompt(type_values: &TypeValues) -> ValueResult {
    let tugger_context_raw = get_context_value(type_values)?;
    let tugger_context = tugger_context_raw
        .downcast_ref::<TuggerContextValue>()
        .ok_or(ValueError::IncorrectParameterType)?;

    Ok(Value::from(tugger_context.can_prompt()))
}

fn starlark_prompt_confirm(
    type_values: &TypeValues,
    prompt: String,
    default: Value,
) -> ValueResult {
    let default = optional_bool_arg("default", &default)?;

    let tugger_context_raw = get_context_value(type_values)?;
    let tugger_context = tugger_context_raw
        .downcast_ref::<TuggerContextValue>()
        .ok_or(ValueError::IncorrectParameterType)?;

    let term = &tugger_context.term_stderr;

    let value = error_context("prompt_confirm()", || {
        if !tugger_context.can_prompt() {
            return prompt_or_default(term, default);
        }

        let mut confirm = Confirm::new();

        let confirm = if let Some(default) = default {
            confirm
                .with_prompt(prompt)
                .default(default)
                .show_default(true)
        } else {
            confirm.with_prompt(prompt)
        };

        if let Some(value) = confirm.interact_on_opt(term)? {
            Ok(value)
        } else {
            Err(anyhow!("confirmation prompt exited; aborting execution"))
        }
    })?;

    Ok(Value::from(value))
}

fn starlark_prompt_input(type_values: &TypeValues, prompt: String, default: Value) -> ValueResult {
    let default = optional_str_arg("default", &default)?;

    let tugger_context_raw = get_context_value(type_values)?;
    let tugger_context = tugger_context_raw
        .downcast_ref::<TuggerContextValue>()
        .ok_or(ValueError::IncorrectParameterType)?;

    let term = &tugger_context.term_stderr;

    let value = error_context("prompt_input()", || {
        if !tugger_context.can_prompt() {
            return prompt_or_default(term, default);
        }

        let mut input = Input::new();

        let input = if let Some(default) = default {
            input
                .with_prompt(prompt)
                .default(default)
                .show_default(true)
        } else {
            input.with_prompt(prompt)
        };

        Ok(input.interact_on(term)?)
    })?;

    Ok(Value::from(value))
}

fn starlark_prompt_password(
    type_values: &TypeValues,
    prompt: String,
    confirm: bool,
    default: Value,
) -> ValueResult {
    let default = optional_str_arg("default", &default)?;

    let tugger_context_raw = get_context_value(type_values)?;
    let tugger_context = tugger_context_raw
        .downcast_ref::<TuggerContextValue>()
        .ok_or(ValueError::IncorrectParameterType)?;

    let term = &tugger_context.term_stderr;

    let password = error_context("prompt_password()", || {
        if !tugger_context.can_prompt() {
            return prompt_or_default(term, default);
        }

        let mut password = Password::new();

        if confirm {
            password
                .with_prompt(prompt)
                .with_confirmation("please confirm", "passwords do not match; please try again")
        } else {
            password.with_prompt(prompt)
        };

        Ok(password.interact_on(term)?)
    })?;

    Ok(Value::from(password))
}

starlark_module! { terminal_module =>
    can_prompt(env env) {
        starlark_can_prompt(env)
    }

    prompt_confirm(env env, prompt: String, default = NoneType::None) {
        starlark_prompt_confirm(env, prompt, default)
    }

    prompt_input(env env, prompt: String, default = NoneType::None) {
        starlark_prompt_input(env, prompt, default)
    }

    prompt_password(env env, prompt: String, confirm: bool = false, default = NoneType::None) {
        starlark_prompt_password(env, prompt, confirm, default)
    }
}

#[cfg(test)]
mod tests {
    use {crate::starlark::testutil::*, anyhow::Result};

    #[test]
    fn can_prompt() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let can_prompt = env.eval("can_prompt()")?;
        assert_eq!(can_prompt.get_type(), "bool");
        assert!(!can_prompt.to_bool());

        Ok(())
    }

    #[test]
    fn prompt_confirm_default() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let value = env.eval("prompt_confirm(\'prompt\', default=True)")?;
        assert_eq!(value.get_type(), "bool");
        assert!(value.to_bool());

        let value = env.eval("prompt_confirm(\'prompt\', default=False)")?;
        assert_eq!(value.get_type(), "bool");
        assert!(!value.to_bool());

        Ok(())
    }

    #[test]
    fn prompt_input_default() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let value = env.eval("prompt_input(\'prompt\', default=\'default\')")?;
        assert_eq!(value.get_type(), "string");
        assert_eq!(value.to_string(), "default");

        Ok(())
    }

    #[test]
    fn prompt_password_default() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let value = env.eval("prompt_password(\'prompt\', default=\'default\')")?;
        assert_eq!(value.get_type(), "string");
        assert_eq!(value.to_string(), "default");

        Ok(())
    }
}
