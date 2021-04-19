// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        environment::Environment,
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
    std::sync::Arc,
};

pub fn get_env() -> Result<Environment> {
    Environment::new()
}

pub fn get_logger() -> Result<slog::Logger> {
    Ok(Logger::root(
        PrintlnDrain {
            min_level: slog::Level::Warning,
        }
        .fuse(),
        slog::o!(),
    ))
}

pub static DISTRIBUTION_CACHE: Lazy<Arc<DistributionCache>> = Lazy::new(|| {
    Arc::new(DistributionCache::new(Some(
        &get_env()
            .expect("failed to resolve environment")
            .python_distributions_dir(),
    )))
});

pub fn get_distribution(
    location: &PythonDistributionLocation,
) -> Result<Arc<StandaloneDistribution>> {
    let env = get_env()?;
    let logger = get_logger()?;

    let dest_path = env.cache_dir().join("python_distributions");

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
