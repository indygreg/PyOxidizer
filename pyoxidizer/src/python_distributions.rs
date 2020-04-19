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
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.8.2-x86_64-unknown-linux-gnu-pgo-20200418T2243.tar.zst".to_string(),
                    sha256: "c7aa51b5deb220e2254a7e32ae7106748d5854b978762f8eb83468c8946dcdbb".to_string(),
                },
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                supports_prebuilt_extension_modules: true,
            },

            // Linux musl.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.8.2-x86_64-unknown-linux-musl-noopt-20200418T2309.tar.zst".to_string(),
                    sha256: "44d6864e5caafb029f94d6d92e5d33f0d1cbc3cb6b14736b4f526609e3a700da".to_string(),
                },
                target_triple: "x86_64-unknown-linux-musl".to_string(),
                supports_prebuilt_extension_modules: false,
            },

            // The order here is important because we will choose the
            // first one.

            // Windows static.
            // TODO re-add these once python-build-standalone produces them.

            // Windows shared.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.8.2-i686-pc-windows-msvc-shared-pgo-20200418T2315.tar.zst".to_string(),
                    sha256: "9b449b079cce7837cd60f1d0d4d0bcbf421018f972555e02f5bd4e219a059220".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.8.2-x86_64-pc-windows-msvc-shared-pgo-20200418T2315.tar.zst".to_string(),
                    sha256: "022b3630265a05475d554ca97d1d85c2d7270cc0c95fb4af1b2155bec7e5bd5d".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },

            // macOS.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.8.2-x86_64-apple-darwin-pgo-20200418T2238.tar.zst".to_string(),
                    sha256: "f6e11a18c3fe841a1a45fc3a786ef54c1540c48aa75b6d47ad8d6ae74b44ce1d".to_string(),
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
