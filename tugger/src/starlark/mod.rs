// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
The `starlark` module and related sub-modules define the
[Starlark](https://github.com/bazelbuild/starlark) dialect used by
Tugger.
*/

pub mod file_resource;
pub mod snapcraft;
#[cfg(test)]
mod testutil;
pub mod wix_bundle_builder;
pub mod wix_installer;
pub mod wix_msi_builder;

use starlark::environment::{Environment, EnvironmentError, TypeValues};

/// Registers Tugger's Starlark dialect.
pub fn register_starlark_dialect(
    env: &mut Environment,
    type_values: &mut TypeValues,
) -> Result<(), EnvironmentError> {
    file_resource::file_resource_module(env, type_values);
    snapcraft::snapcraft_module(env, type_values);
    wix_bundle_builder::wix_bundle_builder_module(env, type_values);
    wix_installer::wix_installer_module(env, type_values);
    wix_msi_builder::wix_msi_builder_module(env, type_values);

    Ok(())
}

/// Populate a Starlark environment with variables needed to support this dialect.
pub fn populate_environment(
    _env: &mut Environment,
    _type_values: &mut TypeValues,
) -> Result<(), EnvironmentError> {
    Ok(())
}
