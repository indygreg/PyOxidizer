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
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.8.8-x86_64-unknown-linux-gnu-pgo-20210228T1503.tar.zst".to_string(),
				sha256: "b31db3a56a1fb203fc77a013ca0c37d68a90bd03da96eb645ceed3cc176ab230".to_string(),
			},
			target_triple: "x86_64-unknown-linux-gnu".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.9.2-x86_64-unknown-linux-gnu-pgo-20210228T1503.tar.zst".to_string(),
				sha256: "a94ee8ccce2e15915bf61077a24454b3a264e0728a74f3fff5d10446cd0d0811".to_string(),
			},
			target_triple: "x86_64-unknown-linux-gnu".to_string(),
			supports_prebuilt_extension_modules: true,
		},

		// Linux musl.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.8.8-x86_64-unknown-linux-musl-noopt-20210228T1503.tar.zst".to_string(),
				sha256: "31ca3fad65ba39071712aa6c1788e787bcc0f2140303927c4957e1395d6bf862".to_string(),
			},
			target_triple: "x86_64-unknown-linux-musl".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.9.2-x86_64-unknown-linux-musl-noopt-20210228T1503.tar.zst".to_string(),
				sha256: "eca125ef4ec230ea3daaed912e506af2bb07510da6309cbbdb3c862677da71e2".to_string(),
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
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.8.8-i686-pc-windows-msvc-shared-pgo-20210228T1503.tar.zst".to_string(),
				sha256: "e1d4cf5fb72b5c613edac4ddf57eb576b7a4f4f2b8562b7c95e06fe4aad6f54d".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.9.2-i686-pc-windows-msvc-shared-pgo-20210228T1503.tar.zst".to_string(),
				sha256: "362a658b295c1161ba744d194254064d8eb36a20424918b0adb459cb74bc07f0".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.8.8-x86_64-pc-windows-msvc-shared-pgo-20210228T1503.tar.zst".to_string(),
				sha256: "5d9c719db2162d9ee48b6b724c8ecfc99c6d6e138799f4e7a2fd9bdefd553fda".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.9.2-x86_64-pc-windows-msvc-shared-pgo-20210228T1503.tar.zst".to_string(),
				sha256: "c960f615036a0ab0f4b608dcf32ff30c6647bf39843e8033fc4b9a56fc3eed7a".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},

		// Windows static.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.8.8-i686-pc-windows-msvc-static-noopt-20210228T1503.tar.zst".to_string(),
				sha256: "eeb7f15559e86da372ba49101e400c06da13da0366c5d393776042f03bceafa1".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.9.2-i686-pc-windows-msvc-static-noopt-20210228T1503.tar.zst".to_string(),
				sha256: "a42c198a55963357fdd9c340de7b4e52ed8a9a92ce459b6c795506f639623816".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.8.8-x86_64-pc-windows-msvc-static-noopt-20210228T1503.tar.zst".to_string(),
				sha256: "42378ca1788018cff9ee077412c3b2fa3d05829377e7f3448d4b9f6cba481ee1".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.9.2-x86_64-pc-windows-msvc-static-noopt-20210228T1503.tar.zst".to_string(),
				sha256: "66cb7aef8f07f6a787f54b7a9c733cdfd89ca8fa8f5cfe643fbae9e5713052b1".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},

		// macOS.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.8.8-x86_64-apple-darwin-pgo-20210228T1503.tar.zst".to_string(),
				sha256: "714ad6215900393528a7fcf38a5266006f613fe506f266ec5b7492ae986aba10".to_string(),
			},
			target_triple: "x86_64-apple-darwin".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210228/cpython-3.9.2-x86_64-apple-darwin-pgo-20210228T1503.tar.zst".to_string(),
				sha256: "bcb19ea30244d01b91a5ac5e51ecf15c0c0bd9b1fc86177d2c595657dc396ef1".to_string(),
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
                "i686-pc-windows-msvc",
                "x86_64-apple-darwin",
                "x86_64-pc-windows-msvc",
                "x86_64-unknown-linux-gnu",
                "x86_64-unknown-linux-musl",
            ]
        );
    }
}
