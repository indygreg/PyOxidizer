// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {anyhow::Result, slog::Drain};

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
