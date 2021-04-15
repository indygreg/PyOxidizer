// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Windows support code. */

mod sdk;
#[cfg(target_family = "windows")]
pub use sdk::find_windows_sdk_current_arch_bin_path;
pub use sdk::target_arch_to_windows_sdk_platform_path;
mod util;
mod vc_redistributable;
pub use vc_redistributable::{
    find_visual_cpp_redistributable, VcRedistributablePlatform, VC_REDIST_ARM64, VC_REDIST_X64,
    VC_REDIST_X86,
};
mod vswhere;
pub use vswhere::find_vswhere;
