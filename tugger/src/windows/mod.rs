// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Windows support code. */

mod util;
mod vc_redistributable;
pub use vc_redistributable::{find_visual_cpp_redistributable, VCRedistributablePlatform};
mod vswhere;
pub use vswhere::find_vswhere;
