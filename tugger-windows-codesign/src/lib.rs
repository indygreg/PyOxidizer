// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Code signing on Windows. */

mod signing;
pub use signing::*;
mod signtool;
pub use signtool::*;
