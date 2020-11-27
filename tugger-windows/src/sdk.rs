// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{anyhow, Result};

#[cfg(target_family = "windows")]
use std::path::PathBuf;

/// Convert the current binary's target architecture to a path used by the Windows SDK.
///
/// This can be used to resolve the path to host-native platform binaries in the
/// Windows SDK.
pub fn target_arch_to_windows_sdk_platform_path() -> Result<&'static str> {
    if cfg!(target_arch = "x86") {
        Ok("x86")
    } else if cfg!(target_arch = "x86_64") {
        Ok("x64")
    } else if cfg!(target_arch = "arm") {
        Ok("arm")
    } else if cfg!(target_arch = "aarch64") {
        Ok("arm64")
    } else {
        Err(anyhow!("target architecture not supported on Windows"))
    }
}

/// Resolve the path to Windows SDK binaries for the current executable's architecture.
///
/// Will return `Err` if the path does not exist.
#[cfg(target_family = "windows")]
pub fn find_windows_sdk_current_arch_bin_path(
    version: Option<find_winsdk::SdkVersion>,
) -> Result<PathBuf> {
    let sdk_info = find_winsdk::SdkInfo::find(version.unwrap_or(find_winsdk::SdkVersion::Any))?
        .ok_or_else(|| anyhow!("could not locate Windows SDK"))?;

    let bin_path = sdk_info.installation_folder().join("bin");

    let candidates = [
        sdk_info.product_version().to_string(),
        format!("{}.0", sdk_info.product_version()),
    ];

    let version_path = candidates
        .iter()
        .filter_map(|p| {
            let p = bin_path.join(p);

            if p.exists() {
                Some(p)
            } else {
                None
            }
        })
        .next()
        .ok_or_else(|| anyhow!("could not locate Windows SDK version path"))?;

    let arch_path = version_path.join(target_arch_to_windows_sdk_platform_path()?);

    if arch_path.exists() {
        Ok(arch_path)
    } else {
        Err(anyhow!("{} does not exist", arch_path.display()))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_family = "windows")]
    use super::*;

    #[cfg(target_family = "windows")]
    #[test]
    pub fn test_find_windows_sdk_current_arch_bin_path() -> Result<()> {
        find_windows_sdk_current_arch_bin_path(None)?;

        Ok(())
    }
}
