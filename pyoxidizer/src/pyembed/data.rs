// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub const FROZEN_IMPORTLIB_NAME: &'static [u8] = b"_frozen_importlib\0";
pub const FROZEN_IMPORTLIB_EXTERNAL_NAME: &'static [u8] = b"_frozen_importlib_external\0";

include!(concat!(env!("OUT_DIR"), "/data.rs"));
