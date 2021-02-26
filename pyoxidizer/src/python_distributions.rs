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

pub static PYTHON_DISTRIBUTIONS: Lazy<PythonDistributionCollection> = Lazy::new(|| {
    let dists = vec![
		// Linux glibc linked.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-unknown-linux-gnu-pgo-20210103T1125.tar.zst".to_string(),
				sha256: "d578b5583cb907eac2a02707e6f43ee0a38b0491f9db5b8e0f23b4f2d3042260".to_string(),
			},
			target_triple: "x86_64-unknown-linux-gnu".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-unknown-linux-gnu-pgo-20210103T1125.tar.zst".to_string(),
				sha256: "cff5b10fab51f9e774bb9e0f9878f1ba2703d005618ce0899905d0a1dac31b45".to_string(),
			},
			target_triple: "x86_64-unknown-linux-gnu".to_string(),
			supports_prebuilt_extension_modules: true,
		},

		// Linux musl.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-unknown-linux-musl-noopt-20210103T1125.tar.zst".to_string(),
				sha256: "5b9f4bcfe550b8ecdb0f74f58b08c2b0fcf7f7ab49d9f0f00865691667d47413".to_string(),
			},
			target_triple: "x86_64-unknown-linux-musl".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-unknown-linux-musl-noopt-20210103T1125.tar.zst".to_string(),
				sha256: "fcf77b7cc208d302abb459ba140ba2668c977499b47817a671405df8ccf0846a".to_string(),
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
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.8.7-i686-pc-windows-msvc-shared-pgo-20210103T1125.tar.zst".to_string(),
				sha256: "c4d577718d02faf508b53458a7039ce6f947897f8be0b689fb138acbf41b496c".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.9.1-i686-pc-windows-msvc-shared-pgo-20210103T1125.tar.zst".to_string(),
				sha256: "e31dbf2fee5f0be6992088b925b8e11c651b7afbcf5111cb888d069bb3273575".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-pc-windows-msvc-shared-pgo-20210103T1125.tar.zst".to_string(),
				sha256: "8fbd66f2ff97192f7c48fd3d00de3330f894531ccbe01205d86b6ed5f4108faa".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-pc-windows-msvc-shared-pgo-20210103T1125.tar.zst".to_string(),
				sha256: "a98f2c0e03d2aeac71d956e3a83c550552d069dde3a4c3b11308504979c5db35".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: true,
		},

		// Windows static.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.8.7-i686-pc-windows-msvc-static-noopt-20210103T1125.tar.zst".to_string(),
				sha256: "03ff84890db1bbf3f6351639c0c682dcc4f7625f88f5f9588cae6f5429d3382c".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.9.1-i686-pc-windows-msvc-static-noopt-20210103T1125.tar.zst".to_string(),
				sha256: "0bd2965f95093a5b89dc44a236d7955ed5d51721658c22cf45580039e0f795af".to_string(),
			},
			target_triple: "i686-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-pc-windows-msvc-static-noopt-20210103T1125.tar.zst".to_string(),
				sha256: "246a071e249a17018c4ace2fe80163fff43891b94193b361f80a0ad69008284e".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-pc-windows-msvc-static-noopt-20210103T1125.tar.zst".to_string(),
				sha256: "b7dea448bebf2b73c5da8f6979f3db4ca17ebdfadfd81f1c67027220b3ef68e5".to_string(),
			},
			target_triple: "x86_64-pc-windows-msvc".to_string(),
			supports_prebuilt_extension_modules: false,
		},

		// macOS.
		PythonDistributionRecord {
			python_major_minor_version: "3.8".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-apple-darwin-pgo-20210103T1125.tar.zst".to_string(),
				sha256: "d76300bb7967b7e5e361a092964d8623141fd5b1e41abae20d7ac6fb87e56c92".to_string(),
			},
			target_triple: "x86_64-apple-darwin".to_string(),
			supports_prebuilt_extension_modules: true,
		},
		PythonDistributionRecord {
			python_major_minor_version: "3.9".to_string(),
			location: PythonDistributionLocation::Url {
				url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-apple-darwin-pgo-20210103T1125.tar.zst".to_string(),
				sha256: "3dac2b81542180e119899c94ea55edc77e3a210a87f56686b38718322e8c6fb5".to_string(),
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
