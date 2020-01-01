// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use starlark::values::{RuntimeError, Value, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE};

pub fn required_type_arg(arg_name: &str, arg_type: &str, value: &Value) -> Result<(), ValueError> {
    let t = value.get_type();
    if t == arg_type {
        Ok(())
    } else {
        Err(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expects a {} for {}; got type {}",
                arg_type, arg_name, t
            ),
            label: format!("expect type {}; got {}", arg_type, t),
        }
        .into())
    }
}

pub fn optional_type_arg(arg_name: &str, arg_type: &str, value: &Value) -> Result<(), ValueError> {
    match value.get_type() {
        "NoneType" => Ok(()),
        _ => required_type_arg(arg_name, arg_type, value),
    }
}

pub fn required_str_arg(name: &str, value: &Value) -> Result<String, ValueError> {
    match value.get_type() {
        "string" => Ok(value.to_str()),
        t => Err(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!("function expects a string for {}; got type {}", name, t),
            label: format!("expected type string; got {}", t),
        }
        .into()),
    }
}

pub fn optional_str_arg(name: &str, value: &Value) -> Result<Option<String>, ValueError> {
    match value.get_type() {
        "NoneType" => Ok(None),
        "string" => Ok(Some(value.to_str())),
        t => Err(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expects an optional string for {}; got type {}",
                name, t
            ),
            label: format!("expected type string; got {}", t),
        }
        .into()),
    }
}

pub fn required_bool_arg(name: &str, value: &Value) -> Result<bool, ValueError> {
    match value.get_type() {
        "bool" => Ok(value.to_bool()),
        t => Err(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expects an optional bool for {}; got type {}",
                name, t
            ),
            label: format!("expected type bool; got {}", t),
        }
        .into()),
    }
}

pub fn required_list_arg(
    arg_name: &str,
    value_type: &str,
    value: &Value,
) -> Result<(), ValueError> {
    match value.get_type() {
        "list" => {
            for v in value.into_iter()? {
                if v.get_type() == value_type {
                    Ok(())
                } else {
                    Err(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!(
                            "list {} expects values of type {}; got {}",
                            arg_name,
                            value_type,
                            v.get_type()
                        ),
                        label: format!("expected type {}; got {}", value_type, v.get_type()),
                    }
                    .into())
                }?;
            }
            Ok(())
        }
        t => Err(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!("function expects a list for {}; got type {}", arg_name, t),
            label: format!("expected type list; got {}", t),
        }
        .into()),
    }
}

pub fn optional_list_arg(
    arg_name: &str,
    value_type: &str,
    value: &Value,
) -> Result<(), ValueError> {
    if value.get_type() == "NoneType" {
        return Ok(());
    }

    required_list_arg(arg_name, value_type, value)
}

pub fn required_dict_arg(
    arg_name: &str,
    key_type: &str,
    value_type: &str,
    value: &Value,
) -> Result<(), ValueError> {
    match value.get_type() {
        "dict" => {
            for k in value.into_iter()? {
                if k.get_type() == key_type {
                    Ok(())
                } else {
                    Err(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!(
                            "dict {} expects keys of type {}; got {}",
                            arg_name,
                            key_type,
                            k.get_type()
                        ),
                        label: format!("expected type {}; got {}", key_type, k.get_type()),
                    }
                    .into())
                }?;

                let v = value.at(k.clone())?;

                if v.get_type() == value_type {
                    Ok(())
                } else {
                    Err(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!(
                            "dict {} expects values of type {}; got {}",
                            arg_name,
                            value_type,
                            v.get_type(),
                        ),
                        label: format!("expected type {}; got {}", value_type, v.get_type()),
                    }
                    .into())
                }?;
            }
            Ok(())
        }
        t => Err(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!("function expects a dict for {}; got type {}", arg_name, t),
            label: format!("expected type dict; got {}", t),
        }
        .into()),
    }
}

pub fn optional_dict_arg(
    arg_name: &str,
    key_type: &str,
    value_type: &str,
    value: &Value,
) -> Result<(), ValueError> {
    if value.get_type() == "NoneType" {
        return Ok(());
    }

    required_dict_arg(arg_name, key_type, value_type, value)
}
