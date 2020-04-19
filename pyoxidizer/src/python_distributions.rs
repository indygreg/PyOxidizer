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
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.7.7-x86_64-unknown-linux-gnu-pgo-20200418T2226.tar.zst".to_string(),
                    sha256: "987ea3f77e168fc71b9c28d7845f76ac61ca804c235caf98c6a674176e7e4dfa".to_string(),
                },
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                supports_prebuilt_extension_modules: true,
            },

            // Linux musl.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.7.7-x86_64-unknown-linux-musl-noopt-20200418T2251.tar.zst".to_string(),
                    sha256: "f0479ce0b6f8e2c752f059673fb15dc07932b836e11926ca6a3cb9ce656b508e".to_string(),
                },
                target_triple: "x86_64-unknown-linux-musl".to_string(),
                supports_prebuilt_extension_modules: false,
            },

            // The order here is important because we will choose the
            // first one.

            // Windows static.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.7.7-i686-pc-windows-msvc-static-noopt-20200418T2317.tar.zst".to_string(),
                    sha256: "7c4a4102677f398c7c39c31141c99f2b3dcaa85651f6f698d379fee372a8a64c".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.7.7-x86_64-pc-windows-msvc-static-noopt-20200418T2311.tar.zst".to_string(),
                    sha256: "820c1eef04eacdba84a1fc1db7ea15fa01791ac6e3ece75546418ef8ac1b1cf3".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },

            // Windows shared.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.7.7-i686-pc-windows-msvc-shared-pgo-20200418T2311.tar.zst".to_string(),
                    sha256: "d78ea86fef04d5ab1e08fa968f24e7e61c8644a1668af592af145bd603deec53".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.7.7-x86_64-pc-windows-msvc-shared-pgo-20200418T2225.tar.zst".to_string(),
                    sha256: "d4ef9027e294fa019e835d23d8e4c6747af043704ec2a0fbb05556179a2b6000".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },

            // macOS.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.7.7-x86_64-apple-darwin-pgo-20200418T2238.tar.zst".to_string(),
                    sha256: "39a936ae7948a4e4237823158633541a23cea66b5fe9c523955de06a45f4f8d6".to_string(),
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
