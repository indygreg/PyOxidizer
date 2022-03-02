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
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "f0ec0e5855a18394d6eaf534ddb07afc22976924b53c30ad0681ffb25f4d1fd4".to_string(),
            },
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "d23017bc20b640615af8f5eab0f1bf0c9264526bcb8c2a326f4a13b21725cff1".to_string(),
            },
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64_v2-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "7125392d62a32146ba30c0b09e8900481ce7066230946c54cf8ff704cfa6a0ba".to_string(),
            },
            target_triple: "x86_64_v2-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64_v3-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "39e3eddef8449b30d5f99c646a79454a2484d6060b8f3102a3def16330e2d2dc".to_string(),
            },
            target_triple: "x86_64_v3-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "28ded83c1742870fe0633619ec43f9b3a693078bf7da1e2afb81a543517e032d".to_string(),
            },
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64_v2-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "8fec24e349ea0cac99e7c3a25616040601a278d85bc5f38cbf5f0ad4b3cef40c".to_string(),
            },
            target_triple: "x86_64_v2-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64_v3-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "39e894485840df5c8468771414751d534059b2c2e4c7f81a46e7c8af858e297e".to_string(),
            },
            target_triple: "x86_64_v3-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },

        // Linux musl.
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "98c6f15464a27f9525e99f324371fd54b380055ed5ff75b0d046e280da36f5f7".to_string(),
            },
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "666217d11cd0d7a1f71afe91ea61b743854ebf046d7df7c1202d41498ad7b979".to_string(),
            },
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64_v2-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "c46c555d1214596a0acc57eecd25297405309f02dd2c68a29ab0cf85f05168e8".to_string(),
            },
            target_triple: "x86_64_v2-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64_v3-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "f9f7b9224876af9cbf7d87580d0c6765f6db838ea28d2b0117b75163d6b0f1df".to_string(),
            },
            target_triple: "x86_64_v3-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "c8844d4ee5caf551bb8f607cc123dd8ad57eb9c5e381be725aeb46b1a59662f6".to_string(),
            },
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64_v2-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "65b7d9a23a1656512f67a7f1e85b2834137335cfa23588b330a9bba1e402c153".to_string(),
            },
            target_triple: "x86_64_v2-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64_v3-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "c62132b7cd27225d3d9822f4bcf06eff817b4f91d9a586371a8b0b3f5a254e3b".to_string(),
            },
            target_triple: "x86_64_v3-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: true,
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
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-i686-pc-windows-msvc-shared-pgo-full.tar.zst".to_string(),
                sha256: "3e2e6c7de78b1924aad37904fed7bfbac6efa2bef05348e9be92180b2f2b1ae1".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-i686-pc-windows-msvc-shared-pgo-full.tar.zst".to_string(),
                sha256: "7f3ca15f89775f76a32e6ea9b2c9778ebf0cde753c5973d4493959e75dd92488".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-i686-pc-windows-msvc-shared-pgo-full.tar.zst".to_string(),
                sha256: "698b09b1b8321a4dc43d62f6230b62adcd0df018b2bcf5f1b4a7ce53dcf23bcc".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst".to_string(),
                sha256: "33f278416ba8074f2ca6d7f8c17b311b60537c9e6431fd47948784c2a78ea227".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst".to_string(),
                sha256: "56b2738599131d03b39b914ea0597862fd9096e5e64816bf19466bf026e74f0c".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst".to_string(),
                sha256: "7397e78a4fbe429144adc1f33af942bdd5175184e082ac88f3023b3a740dd1a0".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },

        // Windows static.
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-i686-pc-windows-msvc-static-noopt-full.tar.zst".to_string(),
                sha256: "672018e7870e31481961fea3a57ddeb645fd80a76847ff8d5bce4cb2cbe3c0f1".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-i686-pc-windows-msvc-static-noopt-full.tar.zst".to_string(),
                sha256: "9f926aa617e28b65944f6bfc790b069772e0a198a4e9774a44d66e33a7253783".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-i686-pc-windows-msvc-static-noopt-full.tar.zst".to_string(),
                sha256: "bc3f8efee1335ad0d07116138b78d60809bea5c2f14f93be22792aec577eda48".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-pc-windows-msvc-static-noopt-full.tar.zst".to_string(),
                sha256: "79fcb90e3beb784f2b5d14efc2d40659b1a88b49f70a5592451a94e94d1df294".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-pc-windows-msvc-static-noopt-full.tar.zst".to_string(),
                sha256: "db833f046af06f9994165fdac4dbf720c15d1c8cd2f152682abb49de6e014e12".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-pc-windows-msvc-static-noopt-full.tar.zst".to_string(),
                sha256: "23e51c5b5568e2898bb0e33181d3a4276ee24ed65260486feb401cd54f310672".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },

        // macOS.
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-aarch64-apple-darwin-pgo-full.tar.zst".to_string(),
                sha256: "9f887882f4f741f60e09c3ba8dd23ada9c1338886a84978b2e021777d51cfc6e".to_string(),
            },
            target_triple: "aarch64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-aarch64-apple-darwin-pgo-full.tar.zst".to_string(),
                sha256: "edf7c4b2e2cfcc7437df93fa6cceb65dcdbf976cdab7718344f0e14351479c5b".to_string(),
            },
            target_triple: "aarch64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-apple-darwin-pgo-full.tar.zst".to_string(),
                sha256: "ccee80383243b899028960417a61b6dc1087c5b8c78ef1678a8b82666d49ac0a".to_string(),
            },
            target_triple: "x86_64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-apple-darwin-pgo-full.tar.zst".to_string(),
                sha256: "a8c5e64256faa625347a417c6f5f1766dce6f1c0a7ae1cb5c7cf07ecc7223170".to_string(),
            },
            target_triple: "x86_64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-apple-darwin-pgo-full.tar.zst".to_string(),
                sha256: "fa40bc33801d6833dac3816fade3420bd5572bbca822c647751f7455931e0663".to_string(),
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
                "x86_64_v2-unknown-linux-gnu",
                "x86_64_v2-unknown-linux-musl",
                "x86_64_v3-unknown-linux-gnu",
                "x86_64_v3-unknown-linux-musl",
            ]
        );
    }
}
