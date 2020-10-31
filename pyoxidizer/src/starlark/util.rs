// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    starlark::values::{none::NoneType, Value},
    std::{os::raw::c_ulong, path::PathBuf},
};

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
