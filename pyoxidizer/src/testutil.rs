// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::py_packaging::distribution::PythonDistribution;
use {
    crate::{
        environment::{default_target_triple, Environment},
        py_packaging::distribution::{
            DistributionCache, DistributionFlavor, PythonDistributionLocation,
        },
        py_packaging::standalone_distribution::StandaloneDistribution,
        python_distributions::PYTHON_DISTRIBUTIONS,
    },
    anyhow::{anyhow, Result},
    once_cell::sync::Lazy,
    std::sync::Arc,
};

static ENVIRONMENT: Lazy<Environment> =
    Lazy::new(|| Environment::new().expect("error spawning global Environment"));

pub fn get_env() -> Result<Environment> {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Info)
        .try_init();

    Ok(ENVIRONMENT.clone())
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

    let dest_path = env.cache_dir().join("python_distributions");

    DISTRIBUTION_CACHE.resolve_distribution(location, Some(&dest_path))
}

pub fn get_default_distribution(
    python_major_minor_version: Option<&str>,
) -> Result<Arc<StandaloneDistribution>> {
    let record = PYTHON_DISTRIBUTIONS
        .find_distribution(
            default_target_triple(),
            &DistributionFlavor::Standalone,
            python_major_minor_version,
        )
        .ok_or_else(|| anyhow!("unable to find distribution"))?;

    get_distribution(&record.location)
}

/// Obtain a suitable host distribution from a target distribution.
pub fn get_host_distribution_from_target(
    target: &Arc<StandaloneDistribution>,
) -> Result<Arc<StandaloneDistribution>> {
    // We have a matching host distribution for each (major-minor, triple) tuple except
    // for 3.8 aarch64-apple-darwin, where we don't have a 3.8 distribution. So in that
    // scenario return a 3.9 distribution.
    let major_minor = if target.python_major_minor_version() == "3.8"
        && default_target_triple() == "aarch64-apple-darwin"
    {
        "3.9".to_string()
    } else {
        target.python_major_minor_version()
    };

    get_default_distribution(Some(major_minor.as_str()))
}

#[cfg(windows)]
pub fn get_default_dynamic_distribution() -> Result<Arc<StandaloneDistribution>> {
    let record = PYTHON_DISTRIBUTIONS
        .find_distribution(
            default_target_triple(),
            &DistributionFlavor::StandaloneDynamic,
            None,
        )
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

/// Obtain all [StandaloneDistribution] in a given chunk.
///
pub fn get_all_standalone_distributions_chunk(
    current_chunk: usize,
    total_chunks: usize,
) -> Result<Vec<Arc<StandaloneDistribution>>> {
    assert!(current_chunk < total_chunks);

    PYTHON_DISTRIBUTIONS
        .iter()
        .enumerate()
        .filter(|(i, _)| i % total_chunks == current_chunk)
        .map(|(_, record)| get_distribution(&record.location))
        .collect::<Result<Vec<_>>>()
}
