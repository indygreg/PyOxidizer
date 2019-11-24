// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::env::optional_str_arg;
use starlark::environment::Environment;
use starlark::values::{default_compare, TypedValue, Value, ValueError, ValueResult};
use starlark::{
    any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
    starlark_signature_extraction, starlark_signatures,
};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TarballDistribution {
    pub distribution: crate::app_packaging::config::DistributionTarball,
}

impl TypedValue for TarballDistribution {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("TarballDistribution<{:#?}>", self.distribution)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "TarballDistribution"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

#[derive(Debug, Clone)]
pub struct WixInstallerDistribution {
    pub distribution: crate::app_packaging::config::DistributionWixInstaller,
}

impl TypedValue for WixInstallerDistribution {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("WixInstallerDistribution<{:#?}>", self.distribution)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "WixInstallerDistribution"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

#[derive(Debug, Clone)]
pub struct Distribution {
    pub distribution: crate::app_packaging::config::Distribution,
}

impl TypedValue for Distribution {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("Distribution<{:#?}>", self.distribution)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "Distribution"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

starlark_module! { distribution_env =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    TarballDistribution(path_prefix=None) {
        let path_prefix = optional_str_arg("path_prefix", &path_prefix)?;

        let distribution = crate::app_packaging::config::DistributionTarball {
            path_prefix,
        };

        Ok(Value::new(TarballDistribution { distribution }))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    WixInstaller(
        msi_upgrade_code_x86=None,
        msi_upgrade_code_amd64=None,
        bundle_upgrade_code=None
    ) {
        let msi_upgrade_code_x86 = optional_str_arg("msi_upgrade_code_x86", &msi_upgrade_code_x86)?;
        let msi_upgrade_code_amd64 = optional_str_arg("msi_upgrade_code_amd64", &msi_upgrade_code_amd64)?;
        let bundle_upgrade_code = optional_str_arg("bundle_upgrade_code", &bundle_upgrade_code)?;

        let distribution = crate::app_packaging::config::DistributionWixInstaller {
            msi_upgrade_code_x86,
            msi_upgrade_code_amd64,
            bundle_upgrade_code,
        };

        Ok(Value::new(WixInstallerDistribution { distribution }))
    }
}
