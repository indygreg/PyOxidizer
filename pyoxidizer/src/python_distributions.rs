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
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200216/cpython-3.7.6-linux64-20200216T2303.tar.zst"),
                sha256: String::from(
                    "58067eecbd1600ea765f7fe7b43562bbc8058db4c84ddbfcaddcd2ee18193907",
                ),
            },
        );

        res.insert(
            "x86_64-unknown-linux-musl".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200217/cpython-3.7.6-linux64-musl-20200218T0557.tar.zst"),
                sha256: String::from(
                    "d5e5d8681b0af13bc3e718a35d6237b3629908a669050cb8c8ab919a731c5718",
                ),
            },
        );

        res.insert(
            "i686-pc-windows-msvc".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200216/cpython-3.7.6-windows-x86-static-20200216T2309.tar.zst"),
                sha256: String::from("29fcca67a022bfac3f29a8a32cb070eef8cecbf052dd9a4eff6feae441ca8fb6"),
            },
        );

        res.insert(
            "x86_64-pc-windows-msvc".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200216/cpython-3.7.6-windows-amd64-static-20200216T2300.tar.zst"),
                sha256: String::from("a9348f50d7289fde92e0ad36073febf9b86448737286cecead08881d009b5829"),
            },
        );

        res.insert(
            "x86_64-apple-darwin".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200216/cpython-3.7.6-macos-20200216T2344.tar.zst"),
                sha256: String::from("0487f70c2b857ddcf8d005ba6de9b97b13800aeaeb505c42da7659d83c79d233"),
            },
        );

        res
    };
    pub static ref CPYTHON_STANDALONE_DYNAMIC_BY_TRIPLE: BTreeMap<String, HostedDistribution> = {
        let mut res: BTreeMap<String, HostedDistribution> = BTreeMap::new();

        res.insert(
            "i686-pc-windows-msvc".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200216/cpython-3.7.6-windows-x86-shared-pgo-20200217T0110.tar.zst"),
                sha256: String::from("a77b2245f0109fa80cd46adeb40815a1e8892002fffa64293a9702f50d547bc2"),
            },
        );

        res.insert(
            "x86_64-pc-windows-msvc".to_string(),
            HostedDistribution {
                url: String::from("https://github.com/indygreg/python-build-standalone/releases/download/20200216/cpython-3.7.6-windows-amd64-shared-pgo-20200217T0022.tar.zst"),
                sha256: String::from("35ccece4950147a9344e4843bc6148882b12b79806707726b15e846eb6cfed4e"),
            },
        );

        res
    };
    pub static ref CPYTHON_WINDOWS_EMBEDDABLE_BY_TRIPLE: BTreeMap<String, HostedDistribution> = {
        let mut res: BTreeMap<String, HostedDistribution> = BTreeMap::new();

        res.insert(
            "i686-pc-windows-msvc".to_string(),
            HostedDistribution {
                url: "https://www.python.org/ftp/python/3.7.6/python-3.7.6-embed-win32.zip"
                    .to_string(),
                sha256: "e2257b87e2e1a131e5d2adf843887fdab5021f8d4d6d68d49691aa965650c3ab"
                    .to_string(),
            },
        );

        res.insert(
            "x86_64-pc-windows-msvc".to_string(),
            HostedDistribution {
                url: "https://www.python.org/ftp/python/3.7.6/python-3.7.6-embed-amd64.zip"
                    .to_string(),
                sha256: "114638061d636285600cbc3d4def64b45c43da9b225cb9eeead30fe7fe7d60d4"
                    .to_string(),
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
