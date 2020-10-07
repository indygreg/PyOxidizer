// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Defines known Python distributions.

use {
    crate::py_packaging::distribution::{
        DistributionFlavor, PythonDistributionLocation, PythonDistributionRecord,
    },
    itertools::Itertools,
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
        self.dists
            .iter()
            .filter(|dist| dist.python_major_minor_version == "3.8")
            .filter(|dist| dist.target_triple == target_triple)
            .filter(|dist| match flavor {
                DistributionFlavor::Standalone => true,
                DistributionFlavor::StandaloneStatic => !dist.supports_prebuilt_extension_modules,
                DistributionFlavor::StandaloneDynamic => dist.supports_prebuilt_extension_modules,
            })
            .cloned()
            .next()
    }

    /// Obtain records for all registered distributions.
    #[allow(unused)]
    pub fn iter(&self) -> impl Iterator<Item = &PythonDistributionRecord> {
        self.dists.iter()
    }

    /// All target triples of distributions in this collection.
    #[allow(unused)]
    pub fn all_target_triples(&self) -> impl Iterator<Item = &str> {
        self.dists
            .iter()
            .map(|dist| dist.target_triple.as_str())
            .sorted()
            .dedup()
    }
}

lazy_static! {
    pub static ref PYTHON_DISTRIBUTIONS: PythonDistributionCollection = {
        let dists = vec![
            // Linux glibc linked.
            PythonDistributionRecord {
                python_major_minor_version: "3.8".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201003/cpython-3.8.6-x86_64-unknown-linux-gnu-pgo-20201003T2016.tar.zst".to_string(),
                    sha256: "897bb37257a2181b64785c4688bc0b29454ddce7a634bbd491d7b59709f11531".to_string(),
                },
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201006/cpython-3.9.0-x86_64-unknown-linux-gnu-pgo-20201007T0146.tar.zst".to_string(),
                    sha256: "ae2019fc4870f77a4eda9f9597b506347ea0cfa05edfea4698c716c31faddca3".to_string(),
                },
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                supports_prebuilt_extension_modules: true,
            },

            // Linux musl.
            PythonDistributionRecord {
                python_major_minor_version: "3.8".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201003/cpython-3.8.6-x86_64-unknown-linux-musl-noopt-20201003T2016.tar.zst".to_string(),
                    sha256: "7bace9a729eb823bc952554ee5dcb91b0e48b6e9717d52b0f44165335546b8df".to_string(),
                },
                target_triple: "x86_64-unknown-linux-musl".to_string(),
                supports_prebuilt_extension_modules: false,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201006/cpython-3.9.0-x86_64-unknown-linux-musl-noopt-20201007T0146.tar.zst".to_string(),
                    sha256: "00f32615b9fa3f804d8b283a158d55addf236dd2b03272ab90549cb801740b27".to_string(),
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
                python_major_minor_version: "3.8".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201003/cpython-3.8.6-i686-pc-windows-msvc-shared-pgo-20201003T2039.tar.zst".to_string(),
                    sha256: "acefe8125a33338b8825c715c9dc49c5a80aa2c9742b1d9b576118ed1852adf8".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201005/cpython-3.9.0-i686-pc-windows-msvc-shared-pgo-20201006T0241.tar.zst".to_string(),
                    sha256: "753df81eb5d5cf50866e2fa023bd3b070ac1569e6fd70e2267267b8dfbd0530f".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.8".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201003/cpython-3.8.6-x86_64-pc-windows-msvc-shared-pgo-20201003T2021.tar.zst".to_string(),
                    sha256: "671122d910e57230df4fe3aae024e8a56613a1786d53567553adc8e31d2490f1".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201005/cpython-3.9.0-x86_64-pc-windows-msvc-shared-pgo-20201006T0240.tar.zst".to_string(),
                    sha256: "471878204860800c6272eb74354aae274bf97ac345401155443e42a3e0788df1".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },

            // Windows static.
            PythonDistributionRecord {
                python_major_minor_version: "3.8".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201003/cpython-3.8.6-i686-pc-windows-msvc-static-noopt-20201003T2034.tar.zst".to_string(),
                    sha256: "12a2ea07b3875228dae67582e97a721e865b1a924efa4dc79aaec1043986ef0b".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201005/cpython-3.9.0-i686-pc-windows-msvc-static-noopt-20201006T0236.tar.zst".to_string(),
                    sha256: "4e722bf60b34212fc24e674f8b981790433a1c367190f99201875126fedc9692".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.8".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201003/cpython-3.8.6-x86_64-pc-windows-msvc-static-noopt-20201003T2015.tar.zst".to_string(),
                    sha256: "4bcbbfc41ca03bb1a6edc1435406f9e27e02f64fc0a0248578a9c26e891c1e39".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201005/cpython-3.9.0-x86_64-pc-windows-msvc-static-noopt-20201006T0232.tar.zst".to_string(),
                    sha256: "79fa7ff6e9e729d22175507826648ab7490d25b5968ad5ad01090497eb4255d8".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },

            // macOS.
            PythonDistributionRecord {
                python_major_minor_version: "3.8".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201003/cpython-3.8.6-x86_64-apple-darwin-pgo-20201003T2017.tar.zst".to_string(),
                    sha256: "aa1b61cceedf3e6661e25de40cc366c91af98a6c2d5a02334d665b59682b02e3".to_string(),
                },
                target_triple: "x86_64-apple-darwin".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201006/cpython-3.9.0-x86_64-apple-darwin-pgo-20201007T0154.tar.zst".to_string(),
                    sha256: "31e00721a776b604b1d4de82a9e8e8a02b09dcb351d51720869301157e67911b".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_target_triples() {
        assert_eq!(
            PYTHON_DISTRIBUTIONS
                .all_target_triples()
                .collect::<Vec<_>>(),
            vec![
                "i686-pc-windows-msvc",
                "x86_64-apple-darwin",
                "x86_64-pc-windows-msvc",
                "x86_64-unknown-linux-gnu",
                "x86_64-unknown-linux-musl",
            ]
        );
    }
}
