// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use starlark::environment::Environment;
use starlark::values::{
    default_compare, RuntimeError, TypedValue, Value, ValueError, ValueResult,
    INCORRECT_PARAMETER_TYPE_ERROR_CODE,
};
use starlark::{
    any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
    starlark_signature_extraction, starlark_signatures,
};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;

use super::env::{optional_str_arg, required_str_arg};
use crate::python_distributions::CPYTHON_BY_TRIPLE;

#[derive(Debug, Clone)]
pub struct PythonDistribution {
    pub source: crate::py_packaging::config::PythonDistribution,
}

impl From<crate::py_packaging::config::PythonDistribution> for PythonDistribution {
    fn from(distribution: crate::py_packaging::config::PythonDistribution) -> Self {
        PythonDistribution {
            source: distribution,
        }
    }
}

impl TypedValue for PythonDistribution {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("PythonDistribution<{:#?}>", self.source)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonDistribution"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

starlark_module! { python_distribution_module =>
    #[allow(non_snake_case)]
    PythonDistribution(sha256, local_path=None, url=None) {
        required_str_arg("sha256", &sha256)?;
        optional_str_arg("local_path", &local_path)?;
        optional_str_arg("url", &url)?;

        if local_path.get_type() != "NoneType" && url.get_type() != "NoneType" {
            return Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "cannot define both local_path and url".to_string(),
                label: "cannot define both local_path and url".to_string(),
            }.into());
        }

        let distribution = if local_path.get_type() != "NoneType" {
            crate::py_packaging::config::PythonDistribution::Local {
                local_path: local_path.to_string(),
                sha256: sha256.to_string(),
            }
        } else {
            crate::py_packaging::config::PythonDistribution::Url {
                url: url.to_string(),
                sha256: sha256.to_string(),
            }
        };

        Ok(Value::new(PythonDistribution::from(distribution)))
    }

    default_python_distribution(env env, build_target=None) {
        let build_target = match build_target.get_type() {
            "NoneType" => env.get("BUILD_TARGET").unwrap().to_string(),
            "string" => build_target.to_string(),
            t => {
                return Err(ValueError::TypeNotX {
                    object_type: t.to_string(),
                    op: "str".to_string(),
                })
            }
        };

        match CPYTHON_BY_TRIPLE.get(&build_target) {
            Some(dist) => {
                let distribution = crate::py_packaging::config::PythonDistribution::Url {
                    url: dist.url.clone(),
                    sha256: dist.sha256.clone(),
                };

                Ok(Value::new(PythonDistribution::from(distribution)))
            }
            None => Err(ValueError::Runtime(RuntimeError {
                code: "no_default_distribution",
                message: format!("could not find default Python distribution for {}", build_target),
                label: "build_target".to_string(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_default_python_distribution() {
        let dist = starlark_ok("default_python_distribution()");
        assert_eq!(dist.get_type(), "PythonDistribution");

        let host_distribution = CPYTHON_BY_TRIPLE
            .get(crate::app_packaging::repackage::HOST)
            .unwrap();

        let wanted = crate::py_packaging::config::PythonDistribution::Url {
            url: host_distribution.url.clone(),
            sha256: host_distribution.sha256.clone(),
        };

        dist.downcast_apply(|x: &PythonDistribution| assert_eq!(x.source, wanted));
    }

    #[test]
    fn test_default_python_distribution_bad_arg() {
        let err = starlark_nok("default_python_distribution(False)");
        assert_eq!(err.message, "The type 'bool' is not str");
    }

    #[test]
    fn test_python_distribution_no_args() {
        let err = starlark_nok("PythonDistribution()");
        assert!(err.message.starts_with("Missing parameter sha256"));
    }

    #[test]
    fn test_python_distribution_multiple_args() {
        let err = starlark_nok(
            "PythonDistribution('sha256', url='url_value', local_path='local_path_value')",
        );
        assert_eq!(err.message, "cannot define both local_path and url");
    }

    #[test]
    fn test_python_distribution_url() {
        let dist = starlark_ok("PythonDistribution('sha256', url='some_url')");
        let wanted = crate::py_packaging::config::PythonDistribution::Url {
            url: "some_url".to_string(),
            sha256: "sha256".to_string(),
        };

        dist.downcast_apply(|x: &PythonDistribution| assert_eq!(x.source, wanted));
    }

    #[test]
    fn test_python_distribution_local_path() {
        let dist = starlark_ok("PythonDistribution('sha256', local_path='some_path')");
        let wanted = crate::py_packaging::config::PythonDistribution::Local {
            local_path: "some_path".to_string(),
            sha256: "sha256".to_string(),
        };

        dist.downcast_apply(|x: &PythonDistribution| assert_eq!(x.source, wanted));
    }
}
