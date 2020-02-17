// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::logging::PrintlnDrain,
    crate::py_packaging::distribution::PythonDistributionLocation,
    crate::py_packaging::standalone_distribution::StandaloneDistribution,
    crate::py_packaging::windows_embeddable_distribution::WindowsEmbeddableDistribution,
    crate::python_distributions::{
        CPYTHON_STANDALONE_DYNAMIC_BY_TRIPLE, CPYTHON_STANDALONE_STATIC_BY_TRIPLE,
        CPYTHON_WINDOWS_EMBEDDABLE_BY_TRIPLE,
    },
    anyhow::Result,
    core::sync::atomic::{AtomicUsize, Ordering},
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
        { tempdir::TempDir::new("pyoxidizer-test").expect("unable to create temp directory") };
    pub static ref DEFAULT_DISTRIBUTION: Arc<Box<StandaloneDistribution>> = {
        let path = DEFAULT_DISTRIBUTION_TEMP_DIR.path();

        let hosted_distribution = CPYTHON_STANDALONE_STATIC_BY_TRIPLE
            .get(env!("HOST"))
            .expect("target triple not supported");

        let logger = get_logger().expect("unable to construct logger");

        let location = PythonDistributionLocation::Url {
            url: hosted_distribution.url.clone(),
            sha256: hosted_distribution.sha256.clone(),
        };

        let dist = StandaloneDistribution::from_location(&logger, &location, path)
            .expect("unable to obtain distribution");

        Arc::new(Box::new(dist))
    };
    pub static ref DEFAULT_DYNAMIC_DISTRIBUTION: Arc<Box<StandaloneDistribution>> = {
        let path = DEFAULT_DISTRIBUTION_TEMP_DIR.path();

        let hosted_distribution = CPYTHON_STANDALONE_DYNAMIC_BY_TRIPLE
            .get(env!("HOST"))
            .expect("target triple not supported");

        let logger = get_logger().expect("unable to construct logger");

        let location = PythonDistributionLocation::Url {
            url: hosted_distribution.url.clone(),
            sha256: hosted_distribution.sha256.clone(),
        };

        let dist = StandaloneDistribution::from_location(&logger, &location, path)
            .expect("unable to obtain distribution");

        Arc::new(Box::new(dist))
    };
    pub static ref DEFAULT_WINDOWS_EMBEDDABLE_DISTRIBUTION: WindowsEmbeddableDistribution = {
        let path = DEFAULT_DISTRIBUTION_TEMP_DIR.path();

        let hosted_distribution = CPYTHON_WINDOWS_EMBEDDABLE_BY_TRIPLE
            .get(env!("HOST"))
            .expect("target triple not supported");

        let logger = get_logger().expect("unable to construct logger");

        let location = PythonDistributionLocation::Url {
            url: hosted_distribution.url.clone(),
            sha256: hosted_distribution.sha256.clone(),
        };

        let dist = WindowsEmbeddableDistribution::from_location(&logger, &location, path)
            .expect("unable to obtain distribution");

        dist
    };
}

static DISTRIBUTION_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn get_default_distribution() -> Result<Arc<Box<StandaloneDistribution>>> {
    Ok(DEFAULT_DISTRIBUTION.clone())
}

pub fn get_default_dynamic_distribution() -> Result<Arc<Box<StandaloneDistribution>>> {
    Ok(DEFAULT_DYNAMIC_DISTRIBUTION.clone())
}

#[allow(unused)]
pub fn get_windows_embeddable_distribution() -> Result<WindowsEmbeddableDistribution> {
    // We need to use a separate distribution per occurrence, otherwise there are race
    // conditions.
    let instance = DISTRIBUTION_COUNTER.fetch_add(1, Ordering::Relaxed);

    let dist_dir = DEFAULT_DISTRIBUTION_TEMP_DIR
        .path()
        .join(format!("windows_embeddable.{}", instance));

    let default_dist_path = DEFAULT_WINDOWS_EMBEDDABLE_DISTRIBUTION
        .python_exe
        .parent()
        .unwrap();

    // Copy files.
    std::fs::create_dir_all(&dist_dir)?;

    for p in std::fs::read_dir(default_dist_path)? {
        let p = p?;

        if p.file_type()?.is_dir() {
            continue;
        }

        let source = p.path();
        let rel = source.strip_prefix(&default_dist_path)?;
        let dest = dist_dir.join(rel);
        std::fs::copy(&source, &dest)?;
    }

    WindowsEmbeddableDistribution::from_directory(&dist_dir)
}
