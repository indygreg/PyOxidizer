// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {anyhow::Result, lazy_static::lazy_static, slog::Drain, std::path::PathBuf};

lazy_static! {
    pub static ref DEFAULT_TEMP_DIR: tempdir::TempDir =
        tempdir::TempDir::new("tugger-test").expect("unable to create temporary directory");
    pub static ref DEFAULT_DOWNLOAD_DIR: PathBuf = {
        let p = if let Ok(manifest_dir) = std::env::var("OUT_DIR") {
            PathBuf::from(manifest_dir).join("tugger-files")
        } else {
            DEFAULT_TEMP_DIR.path().join("tugger-files")
        };

        std::fs::create_dir_all(&p).expect("unable to create download directory");

        p
    };
}

/// A slog Drain that uses println!.
pub struct PrintlnDrain {
    /// Minimum logging level that we're emitting.
    pub min_level: slog::Level,
}

/// slog Drain that uses println!.
impl slog::Drain for PrintlnDrain {
    type Ok = ();
    type Err = std::io::Error;

    fn log(
        &self,
        record: &slog::Record,
        _values: &slog::OwnedKVList,
    ) -> Result<Self::Ok, Self::Err> {
        if record.level().is_at_least(self.min_level) {
            println!("{}", record.msg());
        }

        Ok(())
    }
}

pub fn get_logger() -> Result<slog::Logger> {
    Ok(slog::Logger::root(
        PrintlnDrain {
            min_level: slog::Level::Warning,
        }
        .fuse(),
        slog::o!(),
    ))
}
