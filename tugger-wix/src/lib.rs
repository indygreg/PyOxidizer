// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod bundle_builder;
mod chain;
mod common;
mod exe_package;
mod installer_builder;
mod msi_package;
mod simple_msi_builder;
mod wxs_builder;

pub use {
    bundle_builder::WiXBundleInstallerBuilder,
    chain::ChainElement,
    common::{run_candle, run_light, target_triple_to_wix_arch, write_file_manifest_to_wix},
    exe_package::{Behavior, ExePackage, ExitCode},
    installer_builder::WiXInstallerBuilder,
    msi_package::MsiPackage,
    simple_msi_builder::WiXSimpleMsiBuilder,
    wxs_builder::WxsBuilder,
};
