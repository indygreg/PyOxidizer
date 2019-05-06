// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use lazy_static::lazy_static;
use std::collections::BTreeMap;

/// Describes a Python distribution available at a URL.
pub struct HostedDistribution {
    pub url: String,
    pub sha256: String,
}

lazy_static! {
    pub static ref CPYTHON_BY_TRIPLE: BTreeMap<&'static str, HostedDistribution> = {
        let mut res: BTreeMap<&'static str, HostedDistribution> = BTreeMap::new();

        res.insert("x86_64-unknown-linux-gnu", HostedDistribution {
            url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190505/cpython-3.7.3-linux64-20190506T0025.tar.zst"),
            sha256: String::from("837a685551b48ac3dc40a6b279c20a2ce96b15c2873ba0c537463e188c8e3d1b"),
        });

        res.insert("x86_64-pc-windows-msvc", HostedDistribution {
            url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190505/cpython-3.7.3-windows-amd64-20190506T0003.tar.zst"),
            sha256: String::from("9db612991c1d58b117bb40a9f357d15b75cc9a4b4e476a65cf0ae7ce237be8xii30"),
        });

        res
    };
}
