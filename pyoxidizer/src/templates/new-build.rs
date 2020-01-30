// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Build script to integrate PyOxidizer. */

fn main() {
    for (key, value) in std::env::vars() {
        println!("{}: {}", key, value);
    }
}
