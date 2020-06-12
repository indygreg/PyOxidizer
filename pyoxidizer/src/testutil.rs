// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::logging::PrintlnDrain,
    crate::py_packaging::distribution::DistributionFlavor,
    crate::py_packaging::standalone_distribution::StandaloneDistribution,
    crate::python_distributions::PYTHON_DISTRIBUTIONS,
    anyhow::Result,
    lazy_static::lazy_static,
    slog::{Drain, Logger},
    std::sync::Arc,
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
    pub static ref DEFAULT_DISTRIBUTION: Arc<Box<StandaloneDistribution>> = {
        let path = DEFAULT_DISTRIBUTION_TEMP_DIR.path();

        let dist = PYTHON_DISTRIBUTIONS
            .find_distribution(env!("HOST"), &DistributionFlavor::Standalone)
            .expect("target triple not supported");

        let logger = get_logger().expect("unable to construct logger");

        let dist = StandaloneDistribution::from_location(&logger, &dist.location, path)
            .expect("unable to obtain distribution");

        Arc::new(Box::new(dist))
    };
    pub static ref DEFAULT_DYNAMIC_DISTRIBUTION: Arc<Box<StandaloneDistribution>> = {
        let path = DEFAULT_DISTRIBUTION_TEMP_DIR.path();

        let dist = PYTHON_DISTRIBUTIONS
            .find_distribution(env!("HOST"), &DistributionFlavor::StandaloneDynamic)
            .expect("target triple not supported");

        let logger = get_logger().expect("unable to construct logger");

        let dist = StandaloneDistribution::from_location(&logger, &dist.location, path)
            .expect("unable to obtain distribution");

        Arc::new(Box::new(dist))
    };
}

pub fn get_default_distribution() -> Result<Arc<Box<StandaloneDistribution>>> {
    Ok(DEFAULT_DISTRIBUTION.clone())
}

#[cfg(windows)]
pub fn get_default_dynamic_distribution() -> Result<Arc<Box<StandaloneDistribution>>> {
    Ok(DEFAULT_DYNAMIC_DISTRIBUTION.clone())
}
