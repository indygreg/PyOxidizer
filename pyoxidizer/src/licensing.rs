// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Licensing functionality.

use {log::warn, python_packaging::licensing::LicensedComponents};

/// Log a summary of licensing info.
pub fn log_licensing_info(components: &LicensedComponents) {
    for component in components.license_spdx_components() {
        warn!(
            "{} uses SPDX licenses {}",
            component.flavor(),
            component
                .spdx_expression()
                .expect("should have SPDX expression")
        );
    }

    warn!(
        "All SPDX licenses: {}",
        components.all_spdx_license_names().join(", ")
    );
    for component in components.license_missing_components() {
        warn!("{} lacks a known software license", component.flavor());
    }
    for component in components.license_public_domain_components() {
        warn!("{} is in the public domain", component.flavor());
    }
    for component in components.license_unknown_components() {
        warn!("{} has an unknown software license", component.flavor());
    }
    for component in components.license_copyleft_components() {
        warn!("Component has copyleft license: {}", component.flavor());
    }
}
