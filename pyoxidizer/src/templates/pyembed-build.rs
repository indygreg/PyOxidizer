// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use pyoxidizer::run_from_build;

fn main() {
    // run_from_build() performs the heavy work of embedding Python into
    // this crate's build environment.
    //
    // At build time, this function requires a PyOxidizer config file to
    // be present. Errors will occur if the config file cannot be found
    // or if it is invalid or malformed.
    run_from_build("build.rs");
}
