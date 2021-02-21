// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
	rayon::prelude::*,
    anyhow::{anyhow, Result},
    std::{
        convert::TryFrom,
        fmt::{Display, Formatter},
        path::PathBuf,
    },
};

#[cfg(windows)]
use {crate::find_vswhere, std::collections::BTreeMap};

/// Available VC++ Redistributable platforms we can add to the bundle.
#[derive(Debug, PartialEq)]
pub enum VCRedistributablePlatform {
    X86,
    X64,
    Arm64,
}

impl Display for VCRedistributablePlatform {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::X86 => "x86",
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
        })
    }
}

impl TryFrom<&str> for VCRedistributablePlatform {
    type Error = String;

    fn try_from(value: &str) -> anyhow::Result<Self, Self::Error> {
        match value {
            "x86" => Ok(Self::X86),
            "x64" => Ok(Self::X64),
            "arm64" => Ok(Self::Arm64),
            _ => Err(format!(
                "{} is not a valid platform; use 'x86', 'x64', or 'arm64'",
                value
            )),
        }
    }
}

/// Find the paths to the Visual C++ Redistributable DLLs.
///
/// `redist_version` is the version number of the redistributable. Version `14`
/// is the version for VS2015, 2017, and 2019, which all share the same version.
///
/// The returned paths should have names like `vcruntime140.dll`. Some installs
/// have multiple DLLs.
#[cfg(windows)]
pub fn find_visual_cpp_redistributable(
    redist_version: &str,
    platform: VCRedistributablePlatform,
) -> Result<Vec<PathBuf>> {
    let vswhere_exe = find_vswhere()?;

    let cmd = duct::cmd(
        vswhere_exe,
        vec![
            "-products".to_string(),
            "*".to_string(),
            "-requires".to_string(),
            format!("Microsoft.VisualCPP.Redist.{}.Latest", redist_version),
            "-latest".to_string(),
            "-property".to_string(),
            "installationPath".to_string(),
            "-utf8".to_string(),
        ],
    )
    .stdout_capture()
    .stderr_capture()
    .run()?;

    let install_path = PathBuf::from(
        String::from_utf8(cmd.stdout)?
            .strip_suffix("\r\n")
            .ok_or_else(|| anyhow!("unable to strip string"))?,
    );

    // This gets us the path to the Visual Studio installation root. The vcruntimeXXX.dll
    // files are under a path like: VC\Redist\MSVC\<version>\<arch>\Microsoft.VCXXX.CRT\vcruntimeXXX.dll.

    let paths = glob::glob(
        &install_path
            .join(format!(
                "VC/Redist/MSVC/{}.*/{}/Microsoft.VC*.CRT/vcruntime*.dll",
                redist_version, platform
            ))
            .display()
            .to_string(),
    )?
    .collect::<Vec<_>>()
    .into_par_iter()
    .map(|r| r.map_err(|e| anyhow!("glob error: {}", e)))
    .collect::<Result<Vec<PathBuf>>>()?;

    let mut paths_by_version: BTreeMap<semver::Version, Vec<PathBuf>> = BTreeMap::new();

    for path in paths {
        let stripped = path.strip_prefix(install_path.join("VC").join("Redist").join("MSVC"))?;
        // First path component now is the version number.

        let mut components = stripped.components();
        let version_path = components.next().ok_or_else(|| {
            anyhow!("unable to determine version component (this should not happen)")
        })?;

        paths_by_version
            .entry(semver::Version::parse(
                version_path.as_os_str().to_string_lossy().as_ref(),
            )?)
            .or_insert(vec![])
            .push(path);
    }

    Ok(paths_by_version
        .into_iter()
        .last()
        .ok_or_else(|| anyhow!("unable to find install VC++ Redistributable"))?
        .1)
}

#[cfg(unix)]
pub fn find_visual_cpp_redistributable(
    _version: &str,
    _platform: VCRedistributablePlatform,
) -> Result<Vec<PathBuf>> {
    // TODO we could potentially reference these files at a URL and download them or something.
    Err(anyhow!(
        "Finding the Visual C++ Redistributable is not supported outside of Windows"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_visual_cpp_redistributable_14() -> Result<()> {
        let platforms = vec![
            VCRedistributablePlatform::X86,
            VCRedistributablePlatform::X64,
            VCRedistributablePlatform::Arm64,
        ];

        for platform in platforms {
            let res = find_visual_cpp_redistributable("14", platform);

            if cfg!(windows) {
                if res.is_ok() {
                    println!("found vcruntime files: {:?}", res.unwrap());
                }
            } else {
                assert!(res.is_err());
            }
        }

        Ok(())
    }
}
