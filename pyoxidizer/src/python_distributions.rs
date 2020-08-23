// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Defines known Python distributions.

use {
    crate::py_packaging::distribution::{
        DistributionFlavor, PythonDistributionLocation, PythonDistributionRecord,
    },
    lazy_static::lazy_static,
};

/// Describes a Python distribution available at a URL.
pub struct HostedDistribution {
    pub url: String,
    pub sha256: String,
}

pub struct PythonDistributionCollection {
    dists: Vec<PythonDistributionRecord>,
}

impl PythonDistributionCollection {
    /// Find a Python distribution given a target triple and flavor preference.
    pub fn find_distribution(
        &self,
        target_triple: &str,
        flavor: &DistributionFlavor,
    ) -> Option<PythonDistributionRecord> {
        for dist in &self.dists {
            if dist.target_triple != target_triple {
                continue;
            }

            match flavor {
                DistributionFlavor::Standalone => {
                    return Some(dist.clone());
                }
                DistributionFlavor::StandaloneStatic => {
                    if !dist.supports_prebuilt_extension_modules {
                        return Some(dist.clone());
                    }
                }
                DistributionFlavor::StandaloneDynamic => {
                    if dist.supports_prebuilt_extension_modules {
                        return Some(dist.clone());
                    }
                }
            }
        }

        None
    }
}

lazy_static! {
    pub static ref PYTHON_DISTRIBUTIONS: PythonDistributionCollection = {
        let dists = vec![
            // Linux glibc linked.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200822/cpython-3.8.5-x86_64-unknown-linux-gnu-pgo-20200823T0036.tar.zst".to_string(),
                    sha256: "30841db814a7837780b7161b13ad94e4da5a5425c054cedceb07052abf20c4c2".to_string(),
                },
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                supports_prebuilt_extension_modules: true,
            },

            // Linux musl.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200822/cpython-3.8.5-x86_64-unknown-linux-musl-noopt-20200823T0036.tar.zst".to_string(),
                    sha256: "4ed4cd5de4fd17184079f00522639fdf86ee94f72112c7332048e488d86f0492".to_string(),
                },
                target_triple: "x86_64-unknown-linux-musl".to_string(),
                supports_prebuilt_extension_modules: false,
            },

            // The order here is important because we will choose the
            // first one. We prefer shared distributions on Windows because
            // they are more versatile: statically linked Windows distributions
            // don't declspec(dllexport) Python symbols and can't load shared
            // shared library Python extensions, making them a pain to work
            // with.

            // Windows shared.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200822/cpython-3.8.5-i686-pc-windows-msvc-shared-pgo-20200823T0209.tar.zst".to_string(),
                    sha256: "62bb31b18d4f0e4eafff180ae2f9a26968600a50f53610d7c84bcce13a7e843f".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200822/cpython-3.8.5-x86_64-pc-windows-msvc-shared-pgo-20200823T0130.tar.zst".to_string(),
                    sha256: "7b56968a32801d15ef22acc4e2a509fc9f6fae06e68361abb7566db3e2fb3943".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },

            // Windows static.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200822/cpython-3.8.5-i686-pc-windows-msvc-static-noopt-20200823T0304.tar.zst".to_string(),
                    sha256: "1bc1661b5721d0a652f73f2851c61e190c9e223f4acb9155ab0bcc819b09e121".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200822/cpython-3.8.5-x86_64-pc-windows-msvc-static-noopt-20200823T0237.tar.zst".to_string(),
                    sha256: "9e9acf23d9c4d6eda830b21f49f30df1a110054ef88aead253009921203ab66c".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },

            // macOS.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200823/cpython-3.8.5-x86_64-apple-darwin-pgo-20200823T2228.tar.zst".to_string(),
                    sha256: "5b0d28496cecced067616f46b006f0f193f12d00d6fa3111b4b59180f1ac1c56".to_string(),
                },
                target_triple: "x86_64-apple-darwin".to_string(),
                supports_prebuilt_extension_modules: true,
            },
        ];

        PythonDistributionCollection {
            dists,
        }
    };

    /// Location of source code for get-pip.py, version 19.3.1.
    pub static ref GET_PIP_PY_19: HostedDistribution = {
        HostedDistribution {
            url: "https://github.com/pypa/get-pip/raw/ffe826207a010164265d9cc807978e3604d18ca0/get-pip.py".to_string(),
            sha256: "b86f36cc4345ae87bfd4f10ef6b2dbfa7a872fbff70608a1e43944d283fd0eee".to_string(),
        }
    };
}
