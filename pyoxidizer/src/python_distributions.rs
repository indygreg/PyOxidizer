// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Defines known Python distributions.

use {
    crate::py_packaging::distribution::{DistributionFlavor, PythonDistributionRecord},
    itertools::Itertools,
};

pub use crate::default_python_distributions::PYTHON_DISTRIBUTIONS;

/// Default Python X.Y version to use.
pub const DEFAULT_PYTHON_VERSION: &str = "3.10";

/// A collection of available Python distributions.
pub struct PythonDistributionCollection {
    pub(crate) dists: Vec<PythonDistributionRecord>,
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
        let python_major_minor_version =
            python_major_minor_version.unwrap_or(DEFAULT_PYTHON_VERSION);

        self.dists
            .iter()
            .filter(|dist| dist.python_major_minor_version == python_major_minor_version)
            .filter(|dist| dist.target_triple == target_triple)
            .filter(|dist| match flavor {
                DistributionFlavor::Standalone => true,
                DistributionFlavor::StandaloneStatic => !dist.supports_prebuilt_extension_modules,
                DistributionFlavor::StandaloneDynamic => dist.supports_prebuilt_extension_modules,
            })
            .next().cloned()
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
                "aarch64-unknown-linux-gnu",
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
