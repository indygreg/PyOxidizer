// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::pyrepackager::config::DistributionTarball;
use super::super::pyrepackager::state::BuildContext;
use slog::warn;
use std::path::PathBuf;
use tar;

pub fn produce_tarball(
    logger: &slog::Logger,
    context: &BuildContext,
    config: &DistributionTarball,
) -> Result<(), String> {
    let basename = format!("{}.tar", context.app_name);
    let filename = context.distributions_path.join(basename);

    warn!(logger, "writing tarball to {}", filename.display());

    std::fs::create_dir_all(filename.parent().unwrap()).or_else(|_| {
        Err(format!(
            "unable to create directory for {}",
            filename.display()
        ))
    })?;

    let fh = std::fs::File::create(&filename)
        .or_else(|_| Err(format!("unable to open {} for writing", filename.display())))?;

    let mut builder = tar::Builder::new(fh);
    builder.mode(tar::HeaderMode::Deterministic);

    // The tar crate isn't deterministic when iterating directories. So we
    // do the iteration ourselves.
    let walk =
        walkdir::WalkDir::new(&context.app_path).sort_by(|a, b| a.file_name().cmp(b.file_name()));

    for entry in walk {
        let entry = entry.or_else(|e| Err(e.to_string()))?;

        let path = entry.path();

        if path == context.app_path {
            continue;
        }

        let rel_path = path
            .strip_prefix(&context.app_path)
            .or_else(|_| Err("unable to strip directory prefix".to_string()))?;

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
        builder
            .append_path_with_name(path, &archive_path)
            .or_else(|e| Err(e.to_string()))?;
    }

    builder.finish().or_else(|e| Err(e.to_string()))?;

    Ok(())
}
