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
            url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190713/cpython-3.7.4-linux64-20190713T1809.tar.zst"),
            sha256: String::from("82c27953c8835d1e24fe48f810bfed6cb6f19a234a463802a29ca2a1b56251e8"),
        });

        res.insert("x86_64-unknown-linux-musl", HostedDistribution {
            url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190713/cpython-3.7.4-linux64-musl-20190713T1814.tar.zst"),
            sha256: String::from("0a01a332743f26580772dbcdddf3ff1f65d014a30a68797835cbde9c7b223fee"),
        });

        res.insert("i686-pc-windows-msvc", HostedDistribution {
            url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190713/cpython-3.7.4-windows-x86-20190713T1826.tar.zst"),
            sha256: String::from("5b9cdfb4fe33837fd77810b13a9f91f3a936422faabe7adc96dd9409eb803ccb")
        });

        res.insert(
            "x86_64-pc-windows-msvc",
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190713/cpython-3.7.4-windows-amd64-20190710T0203.tar.zst"),
                sha256: String::from("9d7ced27af3de1e16188f2abb03a368beb7beab9a86b29cbdde1a06ae21d957d"),
            },
        );

        res.insert("x86_64-apple-darwin", HostedDistribution {
            url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20190713/cpython-3.7.4-macos-20190710T0233.tar.zst"),
            sha256: String::from("eec0e971881236baf751588b3d7ae3529c2abee28912427fe08bb50bc69c2890"),
        });

        res
    };
}
