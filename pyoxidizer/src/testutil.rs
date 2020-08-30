// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::logging::PrintlnDrain,
    crate::py_packaging::distribution::{DistributionFlavor, PythonDistributionLocation},
    crate::py_packaging::standalone_distribution::StandaloneDistribution,
    crate::python_distributions::PYTHON_DISTRIBUTIONS,
    anyhow::{anyhow, Result},
    lazy_static::lazy_static,
    slog::{Drain, Logger},
    std::collections::HashMap,
    std::ops::{Deref, DerefMut},
    std::path::PathBuf,
    std::sync::{Arc, Mutex},
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
    static ref CACHED_DISTRIBUTIONS: Mutex<HashMap<PythonDistributionLocation, Arc<Box<StandaloneDistribution>>>> =
        Mutex::new(HashMap::new());
}

pub fn get_distribution(
    location: &PythonDistributionLocation,
) -> Result<Arc<Box<StandaloneDistribution>>> {
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

    let mut lock = CACHED_DISTRIBUTIONS.lock().unwrap();

    if !lock.deref_mut().contains_key(location) {
        let dist = Arc::new(Box::new(StandaloneDistribution::from_location(
            &logger, &location, &dest_path,
        )?));

        lock.deref_mut().insert(location.clone(), dist);
    }

    Ok(lock.deref().get(location).unwrap().clone())
}

pub fn get_default_distribution() -> Result<Arc<Box<StandaloneDistribution>>> {
    let record = PYTHON_DISTRIBUTIONS
        .find_distribution(env!("HOST"), &DistributionFlavor::Standalone)
        .ok_or_else(|| anyhow!("unable to find distribution"))?;

    get_distribution(&record.location)
}

#[cfg(windows)]
pub fn get_default_dynamic_distribution() -> Result<Arc<Box<StandaloneDistribution>>> {
    let record = PYTHON_DISTRIBUTIONS
        .find_distribution(env!("HOST"), &DistributionFlavor::StandaloneDynamic)
        .ok_or_else(|| anyhow!("unable to find distribution"))?;

    get_distribution(&record.location)
}

/// Obtain all `StandaloneDistribution` which are defined.
pub fn get_all_standalone_distributions() -> Result<Vec<Arc<Box<StandaloneDistribution>>>> {
    PYTHON_DISTRIBUTIONS
        .iter()
        .map(|record| get_distribution(&record.location))
        .collect::<Result<Vec<_>>>()
}
