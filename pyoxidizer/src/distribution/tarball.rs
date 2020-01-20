// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{anyhow, Result};
use slog::warn;
use std::path::PathBuf;
use tar;

use crate::app_packaging::config::DistributionTarball;
use crate::app_packaging::state::BuildContext;

pub fn produce_tarball(
    logger: &slog::Logger,
    context: &BuildContext,
    config: &DistributionTarball,
) -> Result<()> {
    let basename = format!("{}.tar", context.app_name);
    let filename = context.distributions_path.join(basename);

    warn!(logger, "writing tarball to {}", filename.display());

    std::fs::create_dir_all(
        filename
            .parent()
            .ok_or_else(|| anyhow!("could not find parent directory"))?,
    )?;

    let fh = std::fs::File::create(&filename)?;

    let mut builder = tar::Builder::new(fh);
    builder.mode(tar::HeaderMode::Deterministic);

    // The tar crate isn't deterministic when iterating directories. So we
    // do the iteration ourselves.
    let walk =
        walkdir::WalkDir::new(&context.app_path).sort_by(|a, b| a.file_name().cmp(b.file_name()));

    for entry in walk {
        let entry = entry?;

        let path = entry.path();

        if path == context.app_path {
            continue;
        }

        let rel_path = path.strip_prefix(&context.app_path)?;

        let archive_path = if let Some(value) = &config.path_prefix {
            PathBuf::from(value).join(rel_path)
        } else {
            PathBuf::from(rel_path)
        };

        warn!(
            logger,
            "adding {} as {}",
            path.display(),
            archive_path.display()
        );
        builder.append_path_with_name(path, &archive_path)?;
    }

    builder.finish()?;

    Ok(())
}
