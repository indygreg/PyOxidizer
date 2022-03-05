// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    pyo3::{ffi as pyffi, prelude::*},
    std::{
        ffi::CStr,
        fmt::{Display, Formatter},
    },
};

/// Format a PyErr in a crude manner.
///
/// This is meant to be called during interpreter initialization. We can't
/// call PyErr_Print() because sys.stdout may not be available yet.
fn format_pyerr(py: Python, err: PyErr) -> Result<String, &'static str> {
    let type_repr = err
        .get_type(py)
        .repr()
        .map_err(|_| "unable to get repr of error type")?;

    let value_repr = err
        .value(py)
        .repr()
        .map_err(|_| "unable to get repr of error value")?;

    let value = format!(
        "{}: {}",
        type_repr.to_string_lossy(),
        value_repr.to_string_lossy()
    );

    Ok(value)
}

/// Represents an error encountered when creating an embedded Python interpreter.
#[derive(Debug)]
pub enum NewInterpreterError {
    Simple(&'static str),
    Dynamic(String),
}

impl From<&'static str> for NewInterpreterError {
    fn from(v: &'static str) -> Self {
        NewInterpreterError::Simple(v)
    }
}

impl Display for NewInterpreterError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self {
            NewInterpreterError::Simple(value) => value.fmt(f),
            NewInterpreterError::Dynamic(value) => value.fmt(f),
        }
    }
}

impl std::error::Error for NewInterpreterError {}

impl NewInterpreterError {
    pub fn new_from_pyerr(py: Python, err: PyErr, context: &str) -> Self {
        match format_pyerr(py, err) {
            Ok(value) => NewInterpreterError::Dynamic(format!("during {}: {}", context, value)),
            Err(msg) => NewInterpreterError::Dynamic(format!("during {}: {}", context, msg)),
        }
    }

    pub fn new_from_pystatus(status: &pyffi::PyStatus, context: &str) -> Self {
        if !status.func.is_null() && !status.err_msg.is_null() {
            let func = unsafe { CStr::from_ptr(status.func) };
            let msg = unsafe { CStr::from_ptr(status.err_msg) };

            NewInterpreterError::Dynamic(format!(
                "during {}: {}: {}",
                context,
                func.to_string_lossy(),
                msg.to_string_lossy()
            ))
        } else if !status.err_msg.is_null() {
            let msg = unsafe { CStr::from_ptr(status.err_msg) };

            NewInterpreterError::Dynamic(format!("during {}: {}", context, msg.to_string_lossy()))
        } else {
            NewInterpreterError::Dynamic(format!("during {}: could not format PyStatus", context))
        }
    }
}
