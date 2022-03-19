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
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.8.13%2B20220318-x86_64-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "9512fcfd1c57b15e60ebebc7425935be778b116b62e716806073f27e72a67d75".to_string(),
            },
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "90f78a741ce04147212219b5e7b1a2359750eee1c16d82f95dbd092dfd98aff5".to_string(),
            },
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64_v2-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "59336abc783c5315c78ba7b26c17db2f7de801ea47d02e677d3bc1d5ad93e877".to_string(),
            },
            target_triple: "x86_64_v2-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64_v3-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "5f376efc50dcc73115f8577da511ac0c48fd33129b61eef3ab6e406e807d04ee".to_string(),
            },
            target_triple: "x86_64_v3-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "65716e4ff95c5598609df6e938ab538fc30241e76df0a03af1946626e20169ca".to_string(),
            },
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64_v2-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "6a006f48efb2a140c9477144863f6ea8a49fc40b343a93af49c755b70c99752f".to_string(),
            },
            target_triple: "x86_64_v2-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64_v3-unknown-linux-gnu-pgo-full.tar.zst".to_string(),
                sha256: "4678880e7485d58b25c325a5b3d2a8673d598e4021dc936cb75da4ba5b8ed316".to_string(),
            },
            target_triple: "x86_64_v3-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },

        // Linux musl.
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.8.13%2B20220318-x86_64-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "9421deebfa4680da331c9b1957e85b079757e302f351f3e0c4002fbf24c8d870".to_string(),
            },
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "741727d71a5eed9f06d202caa2ebc176379054be668493709dcff330fa7f5735".to_string(),
            },
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64_v2-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "792865e1746e693afba3990897c2617c49e8dd6b1b5eb7e1190a2de180a0550a".to_string(),
            },
            target_triple: "x86_64_v2-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64_v3-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "608e25d2223bf82c12267615a4e7d44f2c4b8af8dabc5709349951af9e22b1da".to_string(),
            },
            target_triple: "x86_64_v3-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "70fcc0099c81cb82129278ec3471f952d89293977a94619c92b0a63639dff6bf".to_string(),
            },
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64_v2-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "21290f1e2b732edb227fbb1ae8b66afdd3a0b3c00821ff7afabd263bbc1024e2".to_string(),
            },
            target_triple: "x86_64_v2-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64_v3-unknown-linux-musl-noopt-full.tar.zst".to_string(),
                sha256: "ee37312244c1d6c5ee384f04ddf67b7cdb9aaadaed75116abdf7ebf8a02a0107".to_string(),
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
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.8.13%2B20220318-i686-pc-windows-msvc-shared-pgo-full.tar.zst".to_string(),
                sha256: "177920875eb384f41bcb37cd22d4b956c9a55ca02367e50e37cbc203700f9f21".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-i686-pc-windows-msvc-shared-pgo-full.tar.zst".to_string(),
                sha256: "f06338422e7e3ad25d0cd61864bdb36d565d46440dd363cbb98821d388ed377a".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-i686-pc-windows-msvc-shared-pgo-full.tar.zst".to_string(),
                sha256: "fbc0924a138937fe435fcdb20b0c6241290558e07f158e5578bd91cc8acef469".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.8.13%2B20220318-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst".to_string(),
                sha256: "b11f26fd2ebe7678c97db8896f04a92f4aac354a886087f7aae2395a6dd4a0b4".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst".to_string(),
                sha256: "1fe3c519d43737dc7743aec43f72735e1429c79e06e3901b21bad67b642f1a10".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst".to_string(),
                sha256: "72b91d26f54321ba90a86a3bbc711fa1ac31e0704fec352b36e70b0251ffb13c".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },

        // Windows static.
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.8.13%2B20220318-i686-pc-windows-msvc-static-noopt-full.tar.zst".to_string(),
                sha256: "c42e9418261669646730d4769b83289604234ff9101a197f3bb216035eb02f7b".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-i686-pc-windows-msvc-static-noopt-full.tar.zst".to_string(),
                sha256: "4ee8ea718d7eec3dbb3327b415b618228f3ef30ce29e5d6f3b55ba2248927b59".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-i686-pc-windows-msvc-static-noopt-full.tar.zst".to_string(),
                sha256: "ec694129a76f202fef31b94af50f4a81da6fd8b259c746451ccf6668446c356b".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.8.13%2B20220318-x86_64-pc-windows-msvc-static-noopt-full.tar.zst".to_string(),
                sha256: "83244c610ca9e01896bd73f05e15e51471420872041b8c0db47c66d27ed1ce9b".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-pc-windows-msvc-static-noopt-full.tar.zst".to_string(),
                sha256: "99fe3fb5d0e3af3de4b1703fce2770b7e70936ce3dbc8056a5c4a5542710c9ec".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-pc-windows-msvc-static-noopt-full.tar.zst".to_string(),
                sha256: "8b3368c37b60faf96a40d9d07bd9d18fff42ef69867a9361b175849dde35d1b2".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },

        // macOS.
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-aarch64-apple-darwin-pgo-full.tar.zst".to_string(),
                sha256: "07d64d3dda7e1e99523ac1fd425780cb3f5e4bfb56d67aa27824fc5e46f9ab46".to_string(),
            },
            target_triple: "aarch64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-aarch64-apple-darwin-pgo-full.tar.zst".to_string(),
                sha256: "c76839849146a699cbd5c3714d04b7c664ed0268011556509ec3ebde94e779f4".to_string(),
            },
            target_triple: "aarch64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.8.13%2B20220318-x86_64-apple-darwin-pgo-full.tar.zst".to_string(),
                sha256: "65ef0806ffaea827a674a11fa98c82fd9111076f11400db7b6e3fa43304badb0".to_string(),
            },
            target_triple: "x86_64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-apple-darwin-pgo-full.tar.zst".to_string(),
                sha256: "979a6954f84a804b121a231092872fa8e2472f69fd0523d7580e3d861ac26911".to_string(),
            },
            target_triple: "x86_64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.10".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-apple-darwin-pgo-full.tar.zst".to_string(),
                sha256: "fef0f3c5171d3a1dcb05afebff4e8fc17b3b35ad7fa593afc65e1a4e012309d3".to_string(),
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
