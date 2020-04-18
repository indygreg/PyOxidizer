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
    pub static ref CPYTHON_STANDALONE_STATIC_BY_TRIPLE: BTreeMap<String, HostedDistribution> = {
        let mut res: BTreeMap<String, HostedDistribution> = BTreeMap::new();

        res.insert(
            "x86_64-unknown-linux-gnu".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200408/cpython-3.7.7-linux64-20200409T0045.tar.zst"),
                sha256: String::from(
                    "74799ae3b7f3ddc2d118516d65d46356fb3ef3ff3c4c4591a0dde073c413aff0",
                ),
            },
        );

        res.insert(
            "x86_64-unknown-linux-gnu".to_string(),
            HostedDistribution {
                url: "https://gregoryszorc.com/cpython-3.7.7-x86_64-unknown-linux-gnu-noopt-20200418T1831.tar.zst".to_string(),
                sha256: "79e514bd5e0edef7f031b1d309c53c3e56068390e3d547975b5df077634b23e6".to_string(),
            },
        );

        res.insert(
            "x86_64-unknown-linux-musl".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200408/cpython-3.7.7-linux64-musl-20200409T0047.tar.zst"),
                sha256: String::from(
                    "c1ffa330c7305f46886b7cd2b77edf0e43463113cef426d388476337c3e5cfa9",
                ),
            },
        );

        res.insert(
            "i686-pc-windows-msvc".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200408/cpython-3.7.7-windows-x86-static-20200409T0107.tar.zst"),
                sha256: String::from("978e863fd39f8758c2af18dd750f64eb4b57bd8bfea86cae5fcdde305c56dca7"),
            },
        );

        res.insert(
            "x86_64-pc-windows-msvc".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200408/cpython-3.7.7-windows-amd64-static-20200409T0105.tar.zst"),
                sha256: String::from("fe8d95bc2d7d911ba23c318b786ea7d17c3e2aadbedf47b1d53962bf42e418fe"),
            },
        );

        res.insert(
            "x86_64-apple-darwin".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200408/cpython-3.7.7-macos-20200409T0412.tar.zst"),
                sha256: String::from("f312bea46a7d8efecd4df6b22c03f83016775e6bb5944a5701d697e0a52c63b2"),
            },
        );

        res
    };
    pub static ref CPYTHON_STANDALONE_DYNAMIC_BY_TRIPLE: BTreeMap<String, HostedDistribution> = {
        let mut res: BTreeMap<String, HostedDistribution> = BTreeMap::new();

        res.insert(
            "i686-pc-windows-msvc".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200408/cpython-3.7.7-windows-x86-shared-pgo-20200409T0157.tar.zst"),
                sha256: String::from("69813ae54e691e244e02b25099f7af90a0bac1b63eae80dfe039b7f26010072e"),
            },
        );

        res.insert(
            "x86_64-pc-windows-msvc".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200408/cpython-3.7.7-windows-amd64-shared-pgo-20200409T0115.tar.zst"),
                sha256: String::from("c43c44ebfe9b9f9c59c12481a6233b17bc9a4ad965f8f0dc0063abff4dc59875"),
            },
        );

        res
    };
    /// Location of source code for get-pip.py, version 19.3.1.
    pub static ref GET_PIP_PY_19: HostedDistribution = {
        HostedDistribution {
            url: "https://github.com/pypa/get-pip/raw/ffe826207a010164265d9cc807978e3604d18ca0/get-pip.py".to_string(),
            sha256: "b86f36cc4345ae87bfd4f10ef6b2dbfa7a872fbff70608a1e43944d283fd0eee".to_string(),
        }
    };
}
