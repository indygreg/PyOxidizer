// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::pyrepackager::config::Distribution;
use super::pyrepackager::state::BuildContext;

pub mod tarball;
pub mod wix;

/// Produce distributions from a built application.
pub fn produce_distributions(
    logger: &slog::Logger,
    context: &BuildContext,
    types: &[&str],
) -> Result<(), String> {
    for distribution in &context.config.distributions {
        match distribution {
            Distribution::Tarball(tarball) => {
                if !types.contains(&"tarball") {
                    continue;
                }

                tarball::produce_tarball(logger, context, tarball)?;
            }
            Distribution::WixInstaller(wix) => {
                if !types.contains(&"wix") {
                    continue;
                }

                wix::build_wix_installer(logger, context, wix)?;
            }
        }
    }

    Ok(())
}
