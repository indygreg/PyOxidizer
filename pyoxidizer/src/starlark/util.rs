// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    starlark::values::{
        error::{RuntimeError, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE},
        none::NoneType,
        Value,
    },
    std::{os::raw::c_ulong, path::PathBuf},
};

pub fn required_type_arg(arg_name: &str, arg_type: &str, value: &Value) -> Result<(), ValueError> {
    let t = value.get_type();
    if t == arg_type {
        Ok(())
    } else {
        Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expects a {} for {}; got type {}",
                arg_type, arg_name, t
            ),
            label: format!("expect type {}; got {}", arg_type, t),
        }))
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
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!("function expects a string for {}; got type {}", name, t),
            label: format!("expected type string; got {}", t),
        })),
    }
}

pub fn optional_str_arg(name: &str, value: &Value) -> Result<Option<String>, ValueError> {
    match value.get_type() {
        "NoneType" => Ok(None),
        "string" => Ok(Some(value.to_str())),
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expects an optional string for {}; got type {}",
                name, t
            ),
            label: format!("expected type string; got {}", t),
        })),
    }
}

pub fn required_bool_arg(name: &str, value: &Value) -> Result<bool, ValueError> {
    match value.get_type() {
        "bool" => Ok(value.to_bool()),
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expects an required bool for {}; got type {}",
                name, t
            ),
            label: format!("expected type bool; got {}", t),
        })),
    }
}

pub fn optional_bool_arg(name: &str, value: &Value) -> Result<Option<bool>, ValueError> {
    match value.get_type() {
        "NoneType" => Ok(None),
        "bool" => Ok(Some(value.to_bool())),
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expects an optional bool for {}; got type {}",
                name, t
            ),
            label: format!("expected type bool; got {}", t),
        })),
    }
}

pub fn optional_int_arg(name: &str, value: &Value) -> Result<Option<i64>, ValueError> {
    match value.get_type() {
        "NoneType" => Ok(None),
        "int" => Ok(Some(value.to_int()?)),
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expected an optional int for {}; got type {}",
                name, t
            ),
            label: format!("expected type int; got {}", t),
        })),
    }
}

pub fn required_list_arg(
    arg_name: &str,
    value_type: &str,
    value: &Value,
) -> Result<(), ValueError> {
    match value.get_type() {
        "list" => {
            for v in &value.iter()? {
                if v.get_type() == value_type {
                    Ok(())
                } else {
                    Err(ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!(
                            "list {} expects values of type {}; got {}",
                            arg_name,
                            value_type,
                            v.get_type()
                        ),
                        label: format!("expected type {}; got {}", value_type, v.get_type()),
                    }))
                }?;
            }
            Ok(())
        }
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!("function expects a list for {}; got type {}", arg_name, t),
            label: format!("expected type list; got {}", t),
        })),
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
            for k in &value.iter()? {
                if k.get_type() == key_type {
                    Ok(())
                } else {
                    Err(ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!(
                            "dict {} expects keys of type {}; got {}",
                            arg_name,
                            key_type,
                            k.get_type()
                        ),
                        label: format!("expected type {}; got {}", key_type, k.get_type()),
                    }))
                }?;

                let v = value.at(k.clone())?;

                if v.get_type() == value_type {
                    Ok(())
                } else {
                    Err(ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!(
                            "dict {} expects values of type {}; got {}",
                            arg_name,
                            value_type,
                            v.get_type(),
                        ),
                        label: format!("expected type {}; got {}", value_type, v.get_type()),
                    }))
                }?;
            }
            Ok(())
        }
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!("function expects a dict for {}; got type {}", arg_name, t),
            label: format!("expected type dict; got {}", t),
        })),
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

pub trait ToValue {
    fn to_value(&self) -> Value;
}

impl ToValue for Option<bool> {
    fn to_value(&self) -> Value {
        match self {
            Some(value) => Value::from(*value),
            None => Value::from(NoneType::None),
        }
    }
}

impl ToValue for Option<String> {
    fn to_value(&self) -> Value {
        match self {
            Some(value) => Value::from(value.clone()),
            None => Value::from(NoneType::None),
        }
    }
}

impl ToValue for Option<PathBuf> {
    fn to_value(&self) -> Value {
        match self {
            Some(value) => Value::from(format!("{}", value.display())),
            None => Value::from(NoneType::None),
        }
    }
}

impl ToValue for Option<c_ulong> {
    fn to_value(&self) -> Value {
        match self {
            Some(value) => Value::from((*value) as u64),
            None => Value::from(NoneType::None),
        }
    }
}

impl ToValue for Option<Vec<String>> {
    fn to_value(&self) -> Value {
        match self {
            Some(value) => Value::from(value.clone()),
            None => Value::from(NoneType::None),
        }
    }
}

impl ToValue for Option<Vec<PathBuf>> {
    fn to_value(&self) -> Value {
        match self {
            Some(value) => Value::from(
                value
                    .iter()
                    .map(|x| format!("{}", x.display()))
                    .collect::<Vec<_>>(),
            ),
            None => Value::from(NoneType::None),
        }
    }
}

pub trait ToOptional<T> {
    fn to_optional(&self) -> Option<T>;
}

impl ToOptional<bool> for Value {
    fn to_optional(&self) -> Option<bool> {
        if self.get_type() == "NoneType" {
            None
        } else {
            Some(self.to_bool())
        }
    }
}

impl ToOptional<String> for Value {
    fn to_optional(&self) -> Option<String> {
        if self.get_type() == "NoneType" {
            None
        } else {
            Some(self.to_string())
        }
    }
}

impl ToOptional<PathBuf> for Value {
    fn to_optional(&self) -> Option<PathBuf> {
        if self.get_type() == "NoneType" {
            None
        } else {
            Some(PathBuf::from(self.to_string()))
        }
    }
}

pub trait TryToOptional<T> {
    fn try_to_optional(&self) -> Result<Option<T>, ValueError>;
}

impl TryToOptional<c_ulong> for Value {
    fn try_to_optional(&self) -> Result<Option<c_ulong>, ValueError> {
        if self.get_type() == "NoneType" {
            Ok(None)
        } else {
            Ok(Some(self.to_int()? as c_ulong))
        }
    }
}

impl TryToOptional<Vec<String>> for Value {
    fn try_to_optional(&self) -> Result<Option<Vec<String>>, ValueError> {
        if self.get_type() == "NoneType" {
            Ok(None)
        } else {
            let values = self.to_vec()?;

            Ok(Some(
                values.iter().map(|x| x.to_string()).collect::<Vec<_>>(),
            ))
        }
    }
}

impl TryToOptional<Vec<PathBuf>> for Value {
    fn try_to_optional(&self) -> Result<Option<Vec<PathBuf>>, ValueError> {
        if self.get_type() == "NoneType" {
            Ok(None)
        } else {
            let values = self.to_vec()?;

            Ok(Some(
                values
                    .iter()
                    .map(|x| PathBuf::from(x.to_string()))
                    .collect::<Vec<_>>(),
            ))
        }
    }
}
