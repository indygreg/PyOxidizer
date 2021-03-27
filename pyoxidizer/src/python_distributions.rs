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
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.8.8-x86_64-unknown-linux-gnu-pgo-20210325T0901.tar.zst".to_string(),
				sha256: "0c359b1ba98a0924eb4b78e17ffa91d6dab5c6d5949152c20e57b4b629496fc5".to_string(),
			},
			target_triple: "x86_64-unknown-linux-gnu".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.9.2-x86_64-unknown-linux-gnu-pgo-20210325T0901.tar.zst".to_string(),
				sha256: "6820571678e7f2fb842efc1c8a46184bc3281f396d7878d22b358733b919416e".to_string(),
			},
			target_triple: "x86_64-unknown-linux-gnu".to_string(),
			supports_prebuilt_extension_modules: true,
		},

		// Linux musl.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.8.8-x86_64-unknown-linux-musl-noopt-20210325T0901.tar.zst".to_string(),
				sha256: "bb1bf1cf80160cee08b8ab0322c6efddda9894013ada2be0468ef2b070e4ec03".to_string(),
			},
			target_triple: "x86_64-unknown-linux-musl".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.9.2-x86_64-unknown-linux-musl-noopt-20210325T0901.tar.zst".to_string(),
				sha256: "1ea1f20db1e7a6771f21d8c0da48af96b81e07af45957c3823d6aa3d2f693d5b".to_string(),
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
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.8.8-i686-pc-windows-msvc-shared-pgo-20210325T0901.tar.zst".to_string(),
				sha256: "dd8d90c307127a7997d3dd9f36c77d1d8d36512c333c8f6f43e982b13b9b0dfb".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.9.2-i686-pc-windows-msvc-shared-pgo-20210325T0901.tar.zst".to_string(),
				sha256: "f62864ee01a40fba4ba12f3598ebc93e0024a9fe06489165d1e8dd784c61a4cf".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.8.8-x86_64-pc-windows-msvc-shared-pgo-20210325T0901.tar.zst".to_string(),
				sha256: "f82115919aade69a3e90d7d91575c5cd3191835296d997244605218a6f2c3f9e".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.9.2-x86_64-pc-windows-msvc-shared-pgo-20210325T0901.tar.zst".to_string(),
				sha256: "60a8618f602726bd58c668fe4020043e80d5b93aca9365a88ea3976ae380ff18".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},

		// Windows static.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.8.8-i686-pc-windows-msvc-static-noopt-20210325T0901.tar.zst".to_string(),
				sha256: "a93e884ae7538ecba12ce149eaa7ec23e56e55df717eb0a68ca9a7b217d66cd2".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.9.2-i686-pc-windows-msvc-static-noopt-20210325T0901.tar.zst".to_string(),
				sha256: "43b0b09122b8d2b8966d36506a99ebf87cfe931056bb3d9f94be2758d6e3a2fe".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.8.8-x86_64-pc-windows-msvc-static-noopt-20210325T0901.tar.zst".to_string(),
				sha256: "cb722766ee1572760e03d2a75987e6874d5c642b2943369aa78a1f0bc563631d".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.9.2-x86_64-pc-windows-msvc-static-noopt-20210325T0901.tar.zst".to_string(),
				sha256: "12070464a3c974aa5aa36f5d6d3e05658168af11845f2c10d16fe9c62794222b".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},

		// macOS.
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.9.2-aarch64-apple-darwin-pgo-20210325T0901.tar.zst".to_string(),
				sha256: "0a769ce913ec5e03252c4950c8879fd50855212ec0cc42c14599ab751c987366".to_string(),
			},
			target_triple: "aarch64-apple-darwin".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.8.8-x86_64-apple-darwin-pgo-20210325T0901.tar.zst".to_string(),
				sha256: "6c9cea34b96c7ccf685ae7c4c92bace51a21eb339dca1f9ea5241040040ba9c8".to_string(),
			},
			target_triple: "x86_64-apple-darwin".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210325/cpython-3.9.2-x86_64-apple-darwin-pgo-20210325T0901.tar.zst".to_string(),
				sha256: "5ee5ffb17eba875ec9a9f50b58859ab036d66cf371b2cdd67e364dfc86adf67f".to_string(),
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
