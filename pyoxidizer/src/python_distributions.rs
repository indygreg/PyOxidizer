// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Defines known Python distributions.

use {
    crate::py_packaging::distribution::{
        DistributionFlavor, PythonDistributionLocation, PythonDistributionRecord,
    },
    itertools::Itertools,
    once_cell::sync::Lazy,
};

pub struct PythonDistributionCollection {
    dists: Vec<PythonDistributionRecord>,
}

impl PythonDistributionCollection {
    /// Find a Python distribution given requirements.
    ///
    /// `target_triple` is the Rust machine triple the distribution is built for.
    /// `flavor` is the type of Python distribution.
    /// `python_major_minor_version` is an optional `X.Y` version string being
    /// requested. If `None`, `3.9` is assumed.
    pub fn find_distribution(
        &self,
        target_triple: &str,
        flavor: &DistributionFlavor,
        python_major_minor_version: Option<&str>,
    ) -> Option<PythonDistributionRecord> {
        let python_major_minor_version = python_major_minor_version.unwrap_or("3.9");

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

pub static PYTHON_DISTRIBUTIONS: Lazy<PythonDistributionCollection> = Lazy::new(|| {
    let dists = vec![
        // Linux glibc linked.
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-unknown-linux-gnu-pgo-20210414T1515.tar.zst".to_string(),
                sha256: "70e269473c12f0758ae9d95f6e7ca94de561f4f87cbb304adaaf99297d3a7b5d".to_string(),
            },
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-unknown-linux-gnu-pgo-20210414T1515.tar.zst".to_string(),
                sha256: "60fae850376d18b1282cd76589a4580c7b2bbf45fb263315e43a4ffb35729a19".to_string(),
            },
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },

        // Linux musl.
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-unknown-linux-musl-noopt-20210414T1515.tar.zst".to_string(),
                sha256: "2626710a52cc2384b748252210816f6138749b0625d017f1858c5498fb5f30aa".to_string(),
            },
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-unknown-linux-musl-noopt-20210414T1515.tar.zst".to_string(),
                sha256: "a37c9a26458a4eff40405be1e8368c6e690c0aaaa63ab937e315c4425695c467".to_string(),
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
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-i686-pc-windows-msvc-shared-pgo-20210414T1515.tar.zst".to_string(),
                sha256: "1061523a122911504b525eae1bdf402e4cb129e7658c8fa041bd44032d22dc71".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-i686-pc-windows-msvc-shared-pgo-20210414T1515.tar.zst".to_string(),
                sha256: "a609ee67d1079a425da989806fdc747a7b3ba4244029e72a10b02eb068b63276".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-pc-windows-msvc-shared-pgo-20210414T1515.tar.zst".to_string(),
                sha256: "09f7f405ba86155fded44c0821f05eddebab1008336e0952347e53f09b2d561c".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-pc-windows-msvc-shared-pgo-20210414T1515.tar.zst".to_string(),
                sha256: "ebf4acc5dff238f8adff18e5b60420f05c688896f4020a0b78e5c484190691fa".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },

        // Windows static.
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-i686-pc-windows-msvc-static-noopt-20210414T1515.tar.zst".to_string(),
                sha256: "39ee99d34bbb21b40e376877daca260fd13fc69a91aa19a01985ebf3f4bf48ea".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-i686-pc-windows-msvc-static-noopt-20210414T1515.tar.zst".to_string(),
                sha256: "95f965138a25e98e00228cdc72e48d34260da4b3ad1e0185bb5fbab4ba3b28e4".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-pc-windows-msvc-static-noopt-20210414T1515.tar.zst".to_string(),
                sha256: "fecea6ca47cdfce96e6cac28652d07b5c45f91bf38b4349b51ef40e47e8c3f09".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-pc-windows-msvc-static-noopt-20210414T1515.tar.zst".to_string(),
                sha256: "e5744d8c06a99f541ccb789cc02b0c7149f5538bf241151bbad91a5ff5afc8ba".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },

        // macOS.
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-aarch64-apple-darwin-pgo-20210414T1515.tar.zst".to_string(),
                sha256: "1287d2087056707fc63ecdc2cd0d4ac4b604180b8ae17821dadba08f0cd3f5a3".to_string(),
            },
            target_triple: "aarch64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-apple-darwin-pgo-20210414T1515.tar.zst".to_string(),
                sha256: "0f862f0798ae62a6bb84289361c48ccc52965db449b61ffb79ec6fee302c6130".to_string(),
            },
            target_triple: "x86_64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-apple-darwin-pgo-20210414T1515.tar.zst".to_string(),
                sha256: "00d8693b26b36add162dc57ca6d31212b2a0e315d04f90ab2d15a7ba0f1cd646".to_string(),
            },
            target_triple: "x86_64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
    ];

    PythonDistributionCollection { dists }
});

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
                "aarch64-apple-darwin",
                "i686-pc-windows-msvc",
                "x86_64-apple-darwin",
                "x86_64-pc-windows-msvc",
                "x86_64-unknown-linux-gnu",
                "x86_64-unknown-linux-musl",
            ]
        );
    }
}
