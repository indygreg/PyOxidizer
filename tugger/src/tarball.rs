// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::Result,
    log::warn,
    std::{io::Write, path::Path},
    tar,
};

/// Create a tarball from a filesystem path.
///
/// The uncompressed tar contents will be emitted to the passed writer.
pub fn write_tarball_from_directory<W: Write, P: AsRef<Path>>(
    fh: &mut W,
    source_path: P,
    archive_prefix: Option<P>,
) -> Result<()> {
    let source_path = source_path.as_ref();

    let mut builder = tar::Builder::new(fh);
    builder.mode(tar::HeaderMode::Deterministic);

    // The tar crate isn't deterministic when iterating directories. So we
    // do the iteration ourselves.
    let walk = walkdir::WalkDir::new(source_path).sort_by(|a, b| a.file_name().cmp(b.file_name()));

    for entry in walk {
        let entry = entry?;

        let path = entry.path();

        if path == source_path {
            continue;
        }

        let rel_path = path.strip_prefix(source_path)?;

        let archive_path = if let Some(prefix) = &archive_prefix {
            prefix.as_ref().join(rel_path)
        } else {
            rel_path.to_path_buf()
        };

        warn!("adding {} as {}", path.display(), archive_path.display());
        builder.append_path_with_name(path, &archive_path)?;
    }

    builder.finish()?;

    Ok(())
}
