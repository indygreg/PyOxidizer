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
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-unknown-linux-gnu-pgo-20210327T1202.tar.zst".to_string(),
				sha256: "3b7b1d5c4096e84c5748d4785d6132a19769ec0225f7f9bf856087e7e85d23d7".to_string(),
			},
			target_triple: "x86_64-unknown-linux-gnu".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-unknown-linux-gnu-pgo-20210327T1202.tar.zst".to_string(),
				sha256: "0568a9535804aeb1887785a66b9c7b7ec8b8d30e9a3ad91e870fc60da3f45fe8".to_string(),
			},
			target_triple: "x86_64-unknown-linux-gnu".to_string(),
			supports_prebuilt_extension_modules: true,
		},

		// Linux musl.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-unknown-linux-musl-noopt-20210327T1202.tar.zst".to_string(),
				sha256: "64ca4bddf552f1ec4b4178d7e7186302cd88fdfc006fe1fcaa298a7d17548017".to_string(),
			},
			target_triple: "x86_64-unknown-linux-musl".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-unknown-linux-musl-noopt-20210327T1202.tar.zst".to_string(),
				sha256: "391ffce7cbb0692a6be9dc30d91f6ca5c959709b53ff8a2af537fa417d1b9978".to_string(),
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
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-i686-pc-windows-msvc-shared-pgo-20210327T1202.tar.zst".to_string(),
				sha256: "e30ae5f136e6fd2fb822ee61f699fd800c3aadddd22603fae81197a24b8f00c1".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-i686-pc-windows-msvc-shared-pgo-20210327T1202.tar.zst".to_string(),
				sha256: "258cf1f4887bc161062948c479398b5f6b21c11316d0ce09608a8f894d1c8e4e".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-pc-windows-msvc-shared-pgo-20210327T1202.tar.zst".to_string(),
				sha256: "0c7576adb299eb4b4943589fec517000f4056cfff8d3d8a3d4fe6f957fcd12b4".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-pc-windows-msvc-shared-pgo-20210327T1202.tar.zst".to_string(),
				sha256: "ab812dd3ce372d99db1fc75430998b0bf54d38384dca68ba4b425faa94373378".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},

		// Windows static.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-i686-pc-windows-msvc-static-noopt-20210327T1202.tar.zst".to_string(),
				sha256: "fe024405d4236566a6614724cef338a4e682e82695647d74b933725f7a810fea".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-i686-pc-windows-msvc-static-noopt-20210327T1202.tar.zst".to_string(),
				sha256: "fa4e5f5640d2da564e8479136112b44bf4fa4ed6209b167a99627e6b4f290d5f".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-pc-windows-msvc-static-noopt-20210327T1202.tar.zst".to_string(),
				sha256: "0a218bbfaf5342cdeb22a898ef5b32c0bd2eb2fe20a64df5e57fbd5b9a81b504".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-pc-windows-msvc-static-noopt-20210327T1202.tar.zst".to_string(),
				sha256: "183d54d9767a8a382eded869df29ba16f990991a195c29366c0b706340421b98".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},

		// macOS.
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-aarch64-apple-darwin-pgo-20210327T1202.tar.zst".to_string(),
				sha256: "93bd44a84ea9895a48ce30d30e52e22eb50f65b67bd454044597d1dcfc76c7bc".to_string(),
			},
			target_triple: "aarch64-apple-darwin".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-apple-darwin-pgo-20210327T1202.tar.zst".to_string(),
				sha256: "5e881e9d333ba19b7541d4411dc57b0bb69ef847c9b9ef7ba20ad6ab5d825c66".to_string(),
			},
			target_triple: "x86_64-apple-darwin".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-apple-darwin-pgo-20210327T1202.tar.zst".to_string(),
				sha256: "0c4824d19304a83f96b3f3b6ded85a9a688d5d4c692b1699a6e0e47f741e8956".to_string(),
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
