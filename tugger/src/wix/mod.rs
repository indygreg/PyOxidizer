// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod bundle_builder;
mod common;
mod installer_builder;
mod simple_msi_builder;
mod wxs_builder;

pub use bundle_builder::WiXBundleInstallerBuilder;
pub use common::{run_candle, run_light, target_triple_to_wix_arch, write_file_manifest_to_wix};
pub use installer_builder::WiXInstallerBuilder;
pub use simple_msi_builder::WiXSimpleMSIBuilder;
pub use wxs_builder::WxsBuilder;
