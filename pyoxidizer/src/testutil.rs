// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        logging::PrintlnDrain,
        py_packaging::distribution::{
            DistributionCache, DistributionFlavor, PythonDistributionLocation,
        },
        py_packaging::standalone_distribution::StandaloneDistribution,
        python_distributions::PYTHON_DISTRIBUTIONS,
    },
    anyhow::{anyhow, Result},
    lazy_static::lazy_static,
    slog::{Drain, Logger},
    std::{path::PathBuf, sync::Arc},
};

pub fn get_logger() -> Result<slog::Logger> {
    Ok(Logger::root(
        PrintlnDrain {
            min_level: slog::Level::Warning,
        }
        .fuse(),
        slog::o!(),
    ))
}

lazy_static! {
    pub static ref DEFAULT_DISTRIBUTION_TEMP_DIR: tempdir::TempDir =
        tempdir::TempDir::new("pyoxidizer-test").expect("unable to create temp directory");
    pub static ref DISTRIBUTION_CACHE: Arc<DistributionCache> = Arc::new(DistributionCache::new(
        Some(DEFAULT_DISTRIBUTION_TEMP_DIR.path())
    ));
}

pub fn get_distribution(
    location: &PythonDistributionLocation,
) -> Result<Arc<StandaloneDistribution>> {
    // Use Rust's build directory for distributions if available. This
    // facilitates caching and can make execution much faster.
    // The logic here is far from robust. Perhaps we should add more
    // well-defined and controllable location for storing these files?
    // TODO improve default storage directory detection.
    let dest_path = if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        PathBuf::from(manifest_dir)
            .join("target")
            .join("python_distributions")
    } else {
        DEFAULT_DISTRIBUTION_TEMP_DIR.path().to_path_buf()
    };

    let logger = get_logger()?;

    DISTRIBUTION_CACHE.resolve_distribution(&logger, &location, Some(&dest_path))
}

pub fn get_default_distribution() -> Result<Arc<StandaloneDistribution>> {
    let record = PYTHON_DISTRIBUTIONS
        .find_distribution(env!("HOST"), &DistributionFlavor::Standalone, None)
        .ok_or_else(|| anyhow!("unable to find distribution"))?;

    get_distribution(&record.location)
}

#[cfg(windows)]
pub fn get_default_dynamic_distribution() -> Result<Arc<StandaloneDistribution>> {
    let record = PYTHON_DISTRIBUTIONS
        .find_distribution(env!("HOST"), &DistributionFlavor::StandaloneDynamic, None)
        .ok_or_else(|| anyhow!("unable to find distribution"))?;

    get_distribution(&record.location)
}

/// Obtain all `StandaloneDistribution` which are defined.
pub fn get_all_standalone_distributions() -> Result<Vec<Arc<StandaloneDistribution>>> {
    PYTHON_DISTRIBUTIONS
        .iter()
        .map(|record| get_distribution(&record.location))
        .collect::<Result<Vec<_>>>()
}
