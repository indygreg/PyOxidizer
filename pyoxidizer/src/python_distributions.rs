// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Defines known Python distributions.

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

        res.insert(
            "x86_64-unknown-linux-gnu",
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20191025/cpython-3.7.5-linux64-20191025T0506.tar.zst"),
                sha256: String::from(
                    "608871543e6d2cb80e958638e31158355c578c114e12c77765ea5fb996a5a2c2",
                ),
            },
        );

        res.insert(
            "x86_64-unknown-linux-musl",
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20191025/cpython-3.7.5-linux64-musl-20191026T0603.tar.zst"),
                sha256: String::from(
                    "9d46c1964e32f77f22fec96c8acb905e8d4ff54594ca9a2660467f974dca3a53",
                ),
            },
        );

        res.insert(
            "i686-pc-windows-msvc",
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20191025/cpython-3.7.5-windows-x86-20191025T0549.tar.zst"),
                sha256: String::from("388d37bcffee183bc23f5fec9c263779c59d298d35c9e4445b407d95f94db19c"),
            },
        );

        res.insert(
            "x86_64-pc-windows-msvc",
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20191025/cpython-3.7.5-windows-amd64-20191025T0540.tar.zst"),
                sha256: String::from("86a3260edabeed314c6f32a931e60dd097fa854b1346561443353e1bc90e3edd"),
            },
        );

        res.insert(
            "x86_64-apple-darwin",
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20191025/cpython-3.7.5-macos-20191026T0535.tar.zst"),
                sha256: String::from("e8d0710627c017213d9c5c6496577539a5adceb56d3060e07954ce9bf59f39ae"),
            },
        );

        res
    };
}
