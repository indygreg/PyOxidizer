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
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-unknown-linux-gnu-pgo-20210506T0943.tar.zst".to_string(),
                sha256: "4ec5e5947059b7e1bd5d754711bca62ae26ff6dbebbd2671fae250f94169fcee".to_string(),
            },
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-unknown-linux-gnu-pgo-20210506T0943.tar.zst".to_string(),
                sha256: "be3f8b6e72bad652625c1129158ad553067891bac8c31a53fc0ef5702f7c0eb7".to_string(),
            },
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },

        // Linux musl.
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-unknown-linux-musl-noopt-20210506T0943.tar.zst".to_string(),
                sha256: "11ac204410d58354f27a9af00b5eff95430338e792f2aec5854795d9184fcccf".to_string(),
            },
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-unknown-linux-musl-noopt-20210506T0943.tar.zst".to_string(),
                sha256: "e4f28943fab7a259214c10efeb8a687130b3ed888d7ca50e71902d789db58fd5".to_string(),
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
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-i686-pc-windows-msvc-shared-pgo-20210506T0943.tar.zst".to_string(),
                sha256: "6b7d19fc862a41f7d1d009b231e4b151cff5bbabf08ae72d46fc384ffa40df47".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-i686-pc-windows-msvc-shared-pgo-20210506T0943.tar.zst".to_string(),
                sha256: "d34fed52264e3eeb58dc72432dbe3000b6bdc5b8e1b20f4b01d56db109072a39".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-pc-windows-msvc-shared-pgo-20210506T0943.tar.zst".to_string(),
                sha256: "d0c9d473ff189baee2e214b6fa6ff81ab7865adf85f3360cc95798c4f44a2128".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-pc-windows-msvc-shared-pgo-20210506T0943.tar.zst".to_string(),
                sha256: "4956b8c29ab2841f04cd9aa465e3073be2107ced59636e2a82d35b0c8815e217".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },

        // Windows static.
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-i686-pc-windows-msvc-static-noopt-20210506T0943.tar.zst".to_string(),
                sha256: "399d7af39f2cc3ec420597475e13a89f4827f27c2674e51a2456343ced1f77cd".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-i686-pc-windows-msvc-static-noopt-20210506T0943.tar.zst".to_string(),
                sha256: "4bc5c10b3637eeebb95a3308c8d53e5fde9e00ac05d0d99bb18d2809a8630229".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-pc-windows-msvc-static-noopt-20210506T0943.tar.zst".to_string(),
                sha256: "dbcd1e9ea30b80ea459af525d3da488885f133b322854b11248cb63e2fe9d910".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-pc-windows-msvc-static-noopt-20210506T0943.tar.zst".to_string(),
                sha256: "235f7b7a79aa832a76c05fe52076e26916eed6e42a333d286107c86bb594bbb9".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },

        // macOS.
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-aarch64-apple-darwin-pgo-20210506T0943.tar.zst".to_string(),
                sha256: "10832afecec6c1662229b870497235c239bcf0f221522b41aa77c7fde4f9cc08".to_string(),
            },
            target_triple: "aarch64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-apple-darwin-pgo-20210506T0943.tar.zst".to_string(),
                sha256: "517c1e8ef16bb2117e513a2c4200b57b390f3d6fb02616a49b60be2320626e2a".to_string(),
            },
            target_triple: "x86_64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-apple-darwin-pgo-20210506T0943.tar.zst".to_string(),
                sha256: "27f5ef23c5134a2a4debd8d51aab91842a8e9f195f2bd110d117b8f9f7da8a04".to_string(),
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
