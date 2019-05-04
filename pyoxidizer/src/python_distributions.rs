// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use lazy_static::lazy_static;
use std::collections::HashMap;

/// Describes a Python distribution available at a URL.
pub struct HostedDistribution {
    pub url: String,
    pub sha256: String,
}

lazy_static! {
    static ref CPYTHON_BY_TRIPLE: HashMap<&'static str, HostedDistribution> = {
        let mut res: HashMap<&'static str, HostedDistribution> = HashMap::new();

        res.insert("x86_64-unknown-linux-gnu", HostedDistribution {
            url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190427/cpython-3.7.3-linux64-20190427T2308.tar.zst"),
            sha256: String::from("0b30af0deb4852f2099c7905f80f55b70f7eec152cd19a3a65b577d4350ad47a"),
        });

        res
    };
}
