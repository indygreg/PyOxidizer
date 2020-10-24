// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    starlark::values::{error::ValueError, none::NoneType, Value},
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
