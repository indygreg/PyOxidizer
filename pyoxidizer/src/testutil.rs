// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;
use lazy_static::lazy_static;
use slog::{Drain, Logger};

use crate::logging::PrintlnDrain;
use crate::py_packaging::distribution::{default_distribution, ParsedPythonDistribution};

pub fn get_logger() -> Result<slog::Logger> {
    Ok(Logger::root(
        PrintlnDrain {
            min_level: slog::Level::Error,
        }
        .fuse(),
        slog::o!(),
    ))
}

lazy_static! {
    pub static ref DEFAULT_DISTRIBUTION_TEMP_DIR: tempdir::TempDir =
        { tempdir::TempDir::new("pyoxidizer-test").expect("unable to create temp directory") };
    pub static ref DEFAULT_DISTRIBUTION: ParsedPythonDistribution = {
        let path = DEFAULT_DISTRIBUTION_TEMP_DIR.path();

        let logger = get_logger().expect("unable to construct logger");
        let target = env!("HOST");

        default_distribution(&logger, target, path).expect("unable to obtain distribution")
    };
}

pub fn get_default_distribution() -> Result<ParsedPythonDistribution> {
    Ok(DEFAULT_DISTRIBUTION.clone())
}
