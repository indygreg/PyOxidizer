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
pub mod wix_installer;

use starlark::environment::{Environment, EnvironmentError, TypeValues};

/// Populate a Starlark environment with our dialect.
pub fn populate_environment(
    env: &mut Environment,
    type_values: &mut TypeValues,
) -> Result<(), EnvironmentError> {
    file_resource::file_resource_module(env, type_values);
    snapcraft::snapcraft_module(env, type_values);
    wix_installer::wix_installer_module(env, type_values);

    Ok(())
}
