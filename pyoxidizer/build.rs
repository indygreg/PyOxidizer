// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use vergen::{generate_cargo_keys, ConstantsFlags};

fn main() {
    generate_cargo_keys(ConstantsFlags::all()).expect("error running vergen");

    println!(
        "cargo:rustc-env=HOST={}",
        std::env::var("HOST").expect("HOST not set")
    );
}
