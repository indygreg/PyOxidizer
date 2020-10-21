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
    /// Find a Python distribution given requirements.
    ///
    /// `target_triple` is the Rust machine triple the distribution is built for.
    /// `flavor` is the type of Python distribution.
    /// `python_major_minor_version` is an optional `X.Y` version string being
    /// requested. If `None`, `3.8` is assumed.
    pub fn find_distribution(
        &self,
        target_triple: &str,
        flavor: &DistributionFlavor,
        python_major_minor_version: Option<&str>,
    ) -> Option<PythonDistributionRecord> {
        let python_major_minor_version = python_major_minor_version.unwrap_or("3.8");

        self.dists
            .iter()
            .filter(|dist| dist.python_major_minor_version == python_major_minor_version)
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
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-unknown-linux-gnu-pgo-20201020T0627.tar.zst".to_string(),
                    sha256: "789f58ece3ab4ee599e9fd7f6bd9665157ba1a57dca210739df0687ce3757b55".to_string(),
                },
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-unknown-linux-gnu-pgo-20201020T0627.tar.zst".to_string(),
                    sha256: "d66a271932b763b6aad36a7f23e15263d67248d4828ef90b0278de959938eadc".to_string(),
                },
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                supports_prebuilt_extension_modules: true,
            },

            // Linux musl.
            PythonDistributionRecord {
                python_major_minor_version: "3.8".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-unknown-linux-musl-noopt-20201020T0627.tar.zst".to_string(),
                    sha256: "1dec303ad821b4e54d6562f7d5f85fedf9ac6e7519be60130cdb135c80bd42a5".to_string(),
                },
                target_triple: "x86_64-unknown-linux-musl".to_string(),
                supports_prebuilt_extension_modules: false,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-unknown-linux-musl-noopt-20201020T0627.tar.zst".to_string(),
                    sha256: "bc2fe6bae62c66552f003049501af1bce4ae0fbe5b7411b45e728828e514b5b1".to_string(),
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
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.8.6-i686-pc-windows-msvc-shared-pgo-20201021T0233.tar.zst".to_string(),
                    sha256: "64209ccf373aba15e7577f538e20df9b65bf3c7fb4086d79e2cd5d93ff8ec8fd".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.9.0-i686-pc-windows-msvc-shared-pgo-20201021T0245.tar.zst".to_string(),
                    sha256: "c91d0c7de157b871aaf4a34cfee4008fe6467b8b2a73e0c518f3fcff42c39752".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.8".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-pc-windows-msvc-shared-pgo-20201021T0232.tar.zst".to_string(),
                    sha256: "b842ddc51a3611b574bd5aba8e233bb9a7a52b0479388187f31723ece09bf898".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-pc-windows-msvc-shared-pgo-20201021T0245.tar.zst".to_string(),
                    sha256: "0690cc61ce749b2188cc9380955378a83e2ca95247fe92c754e5c8256c1d32c6".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: true,
            },

            // Windows static.
            PythonDistributionRecord {
                python_major_minor_version: "3.8".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.8.6-i686-pc-windows-msvc-static-noopt-20201021T0259.tar.zst".to_string(),
                    sha256: "eb053159e476915c7d019628b5449bbfb54122838f527a4a37fa48a53ce301cc".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.9.0-i686-pc-windows-msvc-static-noopt-20201021T0302.tar.zst".to_string(),
                    sha256: "0978b73e586d949d762db519f040080797cbc68619f5726353e087febda5ba19".to_string(),
                },
                target_triple: "i686-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.8".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-pc-windows-msvc-static-noopt-20201021T0259.tar.zst".to_string(),
                    sha256: "85454da6108609d6cd2518cad72d7a0023a6a684c7a5f69e9291da9fd2360e32".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-pc-windows-msvc-static-noopt-20201021T0303.tar.zst".to_string(),
                    sha256: "71f5c3b77048dffbfee16791d0018f87a4537e68c9c85d8a9c55367e82c42cf9".to_string(),
                },
                target_triple: "x86_64-pc-windows-msvc".to_string(),
                supports_prebuilt_extension_modules: false,
            },

            // macOS.
            PythonDistributionRecord {
                python_major_minor_version: "3.8".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-apple-darwin-pgo-20201020T0626.tar.zst".to_string(),
                    sha256: "e06807ed4d68928634b2690a86ea7a13d339c0fff3808816e98c646bbdc1f79a".to_string(),
                },
                target_triple: "x86_64-apple-darwin".to_string(),
                supports_prebuilt_extension_modules: true,
            },
            PythonDistributionRecord {
                python_major_minor_version: "3.9".to_string(),
                location: PythonDistributionLocation::Url {
                    url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-apple-darwin-pgo-20201020T0626.tar.zst".to_string(),
                    sha256: "d1ec23459f6eebf881a7c5bf0e77377c67d60add67d57ece6316e425abe69494".to_string(),
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
