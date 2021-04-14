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
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.8.9-x86_64-unknown-linux-gnu-pgo-20210413T2055.tar.zst".to_string(),
                sha256: "69fe80a418cdf9578b5a87b171ffcbf2e0d981a109f0c0f1d871dd90af4e09be".to_string(),
            },
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-unknown-linux-gnu-pgo-20210413T2055.tar.zst".to_string(),
                sha256: "2ac77d2ff86fca72e7fc283a20b2c1d7c02dc953f65ce834fd01e17b0836b23b".to_string(),
            },
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            supports_prebuilt_extension_modules: true,
        },

        // Linux musl.
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.8.9-x86_64-unknown-linux-musl-noopt-20210413T2055.tar.zst".to_string(),
                sha256: "a6207e91236b061d20035a86071867927b984d268d9a7fdfbe1b8da1a939dcc4".to_string(),
            },
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-unknown-linux-musl-noopt-20210413T2055.tar.zst".to_string(),
                sha256: "8d88883ddd677efcbff96b458978d76e6c54810767ea25bac59cb9084b0ed7b2".to_string(),
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
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.8.9-i686-pc-windows-msvc-shared-pgo-20210413T2055.tar.zst".to_string(),
                sha256: "93bf749706ecb9b1d2fcde65db4e52dceb937eeb669b172625e8f555cd9d35ab".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-i686-pc-windows-msvc-shared-pgo-20210413T2055.tar.zst".to_string(),
                sha256: "2b213b468295b059936cf9293cf06a5fa987ec589efec2e5d780e047d1288b07".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.8.9-x86_64-pc-windows-msvc-shared-pgo-20210413T2055.tar.zst".to_string(),
                sha256: "52d0822aa8eac29c7bdff41a6b886fa9f984152a1809030f1379d87ce4462c95".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-pc-windows-msvc-shared-pgo-20210413T2055.tar.zst".to_string(),
                sha256: "d4877279e4efe20be430624346c83d82db7ef4c379b249ad829b974ddf0f1f86".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: true,
        },

        // Windows static.
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.8.9-i686-pc-windows-msvc-static-noopt-20210413T2055.tar.zst".to_string(),
                sha256: "432fe6ad4274da9e4782d5a1503383505e8ef990b9d4c4d903edbea65028316c".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-i686-pc-windows-msvc-static-noopt-20210413T2055.tar.zst".to_string(),
                sha256: "ae18c20cdb956a229f3d89498d6662180c5ad1d396aecd212bd8b2e59412f3e8".to_string(),
            },
            target_triple: "i686-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.8.9-x86_64-pc-windows-msvc-static-noopt-20210413T2055.tar.zst".to_string(),
                sha256: "f74e99128689a25a899274e88258d440420c85b6ee505e0732cdbabbb1092394".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-pc-windows-msvc-static-noopt-20210413T2055.tar.zst".to_string(),
                sha256: "f52f87b19e578db5a9d139ef133df506fecba1e5f4d5e2b556190703043471c7".to_string(),
            },
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            supports_prebuilt_extension_modules: false,
        },

        // macOS.
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-aarch64-apple-darwin-pgo-20210413T2055.tar.zst".to_string(),
                sha256: "632fa81c5b68059b87ede17901630d771b311d4428d21eca6f12bebb2e77453a".to_string(),
            },
            target_triple: "aarch64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.8".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.8.9-x86_64-apple-darwin-pgo-20210413T2055.tar.zst".to_string(),
                sha256: "f5353447d7f5f8bf391750bfb657921f6034c7a4796ba6024b59113042b54404".to_string(),
            },
            target_triple: "x86_64-apple-darwin".to_string(),
            supports_prebuilt_extension_modules: true,
        },
        PythonDistributionRecord {
            python_major_minor_version: "3.9".to_string(),
            location: PythonDistributionLocation::Url {
                url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-apple-darwin-pgo-20210413T2055.tar.zst".to_string(),
                sha256: "cd7413839200c1f459bbb1aec7537afa1e0a3b29105b8be4605573308d337b1f".to_string(),
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
