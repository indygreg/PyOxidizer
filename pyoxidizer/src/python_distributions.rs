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

        res.insert("x86_64-unknown-linux-gnu", HostedDistribution {
            url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190617/cpython-3.7.3-linux64-20190618T0324.tar.zst"),
            sha256: String::from("d6b80a9723c124d6d193f8816fdb874ba6d56abfb35cbfcc2b27de53176d0620"),
        });

        res.insert("x86_64-unknown-linux-musl", HostedDistribution {
            url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190617/cpython-3.7.3-linux64-musl-20190618T0400.tar.zst"),
            sha256: String::from("2be2d109b82634b36685b89800887501b619ef946dda182e5a8ab5c7029a8136"),
        });

        res.insert(
            "x86_64-pc-windows-msvc",
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190617/cpython-3.7.3-windows-amd64-20190618T0516.tar.zst"),
                sha256: String::from("fd43554b5654a914846cf1c251d1ad366f46c7c4d20b7c44572251b533351221"),
            },
        );

        res.insert("x86_64-apple-darwin", HostedDistribution {
            url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190617/cpython-3.7.3-macos-20190618T0523.tar.zst"),
            sha256: String::from("6668202a3225892ce252eff4bb53a58ac058b6a413ab9d37c026a500c2a561ee"),
        });

        res
    };
}
