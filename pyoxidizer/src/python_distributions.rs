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
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190816/cpython-3.7.4-linux64-20190817T0224.tar.zst"),
                sha256: String::from(
                    "1d3b5dc07ee2ddbb5e07bb3f737f368ea0ada088801e1e47d1f12f29cea6a851",
                ),
            },
        );

        res.insert(
            "x86_64-unknown-linux-musl",
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190816/cpython-3.7.4-linux64-musl-20190817T0227.tar.zst"),
                sha256: String::from(
                    "1b20b339fa38aa93b47f754c612fd544a4b82949b51a20b0523430b3abf1d156",
                ),
            },
        );

        res.insert(
            "i686-pc-windows-msvc",
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190816/cpython-3.7.4-windows-x86-20190817T0235.tar.zst"),
                sha256: String::from("46c77de57c5ebbcc8c25e1003a77ca827763da01455b097f4e5fc0b782526c9b"),
            },
        );

        res.insert(
            "x86_64-pc-windows-msvc",
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190816/cpython-3.7.4-windows-amd64-20190817T0227.tar.zst"),
                sha256: String::from("82ae15f31178c9854bacb5d59e00305c6f6080649c9960a29be6b92517b8e5e5"),
            },
        );

        res.insert(
            "x86_64-apple-darwin",
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190816/cpython-3.7.4-macos-20190817T0220.tar.zst"),
                sha256: String::from(
                    "4a77d5ca898196bbc977eb126d129340bab14fb6a8feaaa335675613852071de",
                ),
            },
        );

        res
    };
}
