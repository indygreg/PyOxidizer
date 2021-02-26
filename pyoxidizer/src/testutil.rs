// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        environment::BUILD_GIT_REPO_PATH,
        logging::PrintlnDrain,
        py_packaging::distribution::{
            DistributionCache, DistributionFlavor, PythonDistributionLocation,
        },
        py_packaging::standalone_distribution::StandaloneDistribution,
        python_distributions::PYTHON_DISTRIBUTIONS,
    },
    anyhow::{anyhow, Result},
    once_cell::sync::Lazy,
    slog::{Drain, Logger},
    std::{ops::Deref, path::PathBuf, sync::Arc},
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

pub static DEFAULT_DISTRIBUTION_TEMP_DIR: Lazy<tempfile::TempDir> = Lazy::new(|| {
    tempfile::Builder::new()
        .prefix("pyoxidizer-test")
        .tempdir()
        .expect("unable to create temp directory")
});
pub static DISTRIBUTION_CACHE: Lazy<Arc<DistributionCache>> = Lazy::new(|| {
    Arc::new(DistributionCache::new(Some(
        DEFAULT_DISTRIBUTION_TEMP_DIR.path(),
    )))
});

pub fn get_distribution(
    location: &PythonDistributionLocation,
) -> Result<Arc<StandaloneDistribution>> {
    let dest_path = if let Some(build_path) = &BUILD_GIT_REPO_PATH.deref() {
        build_path.join("target").join("python_distributions")
    } else if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
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
