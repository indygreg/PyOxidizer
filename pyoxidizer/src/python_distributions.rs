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
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.8.8-x86_64-unknown-linux-gnu-pgo-20210303T0937.tar.zst".to_string(),
				sha256: "ed52b81393b18d273ff41dd543a8e69b4db7c57af52043ca075ec8fd14de807c".to_string(),
			},
			target_triple: "x86_64-unknown-linux-gnu".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.9.2-x86_64-unknown-linux-gnu-pgo-20210303T0937.tar.zst".to_string(),
				sha256: "3d044670dfed6feae8c8955823c083121ac5358b295b85f5b8fffa3bfbf0cb08".to_string(),
			},
			target_triple: "x86_64-unknown-linux-gnu".to_string(),
			supports_prebuilt_extension_modules: true,
		},

		// Linux musl.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.8.8-x86_64-unknown-linux-musl-noopt-20210303T0937.tar.zst".to_string(),
				sha256: "beba174adf7359303b648d8309cdf143a45f214f368734d689a2a6be0613dcf4".to_string(),
			},
			target_triple: "x86_64-unknown-linux-musl".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.9.2-x86_64-unknown-linux-musl-noopt-20210303T0937.tar.zst".to_string(),
				sha256: "029b44fa06192745c0c0f28342b68b513b33e0b413047258d43d094ccd03888a".to_string(),
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
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.8.8-i686-pc-windows-msvc-shared-pgo-20210303T0937.tar.zst".to_string(),
				sha256: "87e60e19e9afb76ceb8c6e91405c422d7131f52eddb8b67512e8981385dfba32".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.9.2-i686-pc-windows-msvc-shared-pgo-20210303T0937.tar.zst".to_string(),
				sha256: "a7b15741608b7b406122bd23c2862e6f5cf7b06eecfb97af4f9d26895c2c9c63".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.8.8-x86_64-pc-windows-msvc-shared-pgo-20210303T0937.tar.zst".to_string(),
				sha256: "661c3abdc2086c9f9377182b3316de579ee4fa4d9011cc7dae62fe58930df1f5".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.9.2-x86_64-pc-windows-msvc-shared-pgo-20210303T0937.tar.zst".to_string(),
				sha256: "27b27d19eb6a460fb865547a98d30f76fe1875daec4413fa7752c35a53195c01".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},

		// Windows static.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.8.8-i686-pc-windows-msvc-static-noopt-20210303T0937.tar.zst".to_string(),
				sha256: "30924bf1b0bf3666fdd0ffa2bec2609fae18440abdcd56157c2e00a0bb754c4e".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.9.2-i686-pc-windows-msvc-static-noopt-20210303T0937.tar.zst".to_string(),
				sha256: "5bbd95f105fc9d5b20b1b923a349a5ab0baf45ac8718e6a7742af91e082e799a".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.8.8-x86_64-pc-windows-msvc-static-noopt-20210303T0937.tar.zst".to_string(),
				sha256: "7a819b16b5495d2bd531b520753297848213b790287bca81bbddf62fc6a014ae".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.9.2-x86_64-pc-windows-msvc-static-noopt-20210303T0937.tar.zst".to_string(),
				sha256: "fc88c7c77b99d31a4c887feb7362b34100156f97e90a138ff0288b928520845e".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},

		// macOS.
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.9.2-aarch64-apple-darwin-pgo-20210303T0937.tar.zst".to_string(),
				sha256: "86a4fe500533cf8676ea1cf35571bc92b31b7e09ab54d00a42fd52f65e233f84".to_string(),
			},
			target_triple: "aarch64-apple-darwin".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.8.8-x86_64-apple-darwin-pgo-20210303T0937.tar.zst".to_string(),
				sha256: "b87aba6f6c6abed1365cee5b73e120fc1beca35414b535ffe944136943bdc8a3".to_string(),
			},
			target_triple: "x86_64-apple-darwin".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210303/cpython-3.9.2-x86_64-apple-darwin-pgo-20210303T0937.tar.zst".to_string(),
				sha256: "bba27f89e6ec4777d8650a0e7c413f86c680fbf5e2b75f2ecc23913af5d40b65".to_string(),
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
