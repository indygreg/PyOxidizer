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
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.8.3-x86_64-unknown-linux-gnu-pgo-20200518T0040.tar.zst".to_string(),
                    sha256: "a9aee0f0bd2f8aab09b386915daea508e6713ad43a45fa13afe43fd3e1b1fd9b".to_string(),
                },
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                supports_prebuilt_extension_modules: true,
            },

            // Linux musl.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.8.3-x86_64-unknown-linux-musl-noopt-20200518T0040.tar.zst".to_string(),
                    sha256: "0feb2e51b65a9608b4687d6d37ec1ddf3cda26408655de65306f25121eace6c0".to_string(),
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
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.8.3-i686-pc-windows-msvc-shared-pgo-20200518T0154.tar.zst".to_string(),
                    sha256: "5293cc4f247ac26f4a4be101cc7562e53a43896a33ed464cd0bd31ef760a89d9".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.8.3-x86_64-pc-windows-msvc-shared-pgo-20200517T2207.tar.zst".to_string(),
                    sha256: "da40fadb58d91358c05093220fad201a42ceac320244667b00715d0cd57208c2".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },

            // Windows static.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.8.3-i686-pc-windows-msvc-static-noopt-20200517T2247.tar.zst".to_string(),
                    sha256: "2b7857e66d00068e407a82e737d19156ec24e9c6808b71170244e8707b3e8bed".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.8.3-x86_64-pc-windows-msvc-static-noopt-20200517T2203.tar.zst".to_string(),
                    sha256: "a5357691aafb186c65e7736e9c21a4ba47cb675a25d89ac65be320698f72fd9e".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },

            // macOS.
            PythonDistributionRecord {
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20200530/cpython-3.8.3-x86_64-apple-darwin-pgo-20200530T1845.tar.zst".to_string(),
                    sha256: "adf98af0f0ba8f55a84476e0800210b59edd67bb98800be3ebc5d1f0157ff01e".to_string(),
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
