// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    once_cell::sync::Lazy,
    std::{
        convert::TryFrom,
        fmt::{Display, Formatter},
        path::PathBuf,
    },
    tugger_common::http::RemoteContent,
};

#[cfg(windows)]
use {crate::find_vswhere, std::collections::BTreeMap};

// Latest versions of the VC++ Redistributable can be found at
// https://support.microsoft.com/en-us/help/2977003/the-latest-supported-visual-c-downloads.
// The download URL will redirect to a deterministic artifact, which is what we
// record here.

pub static VC_REDIST_X86: Lazy<RemoteContent> = Lazy::new(|| {
    RemoteContent {
        url: "https://download.visualstudio.microsoft.com/download/pr/d64b93c3-f270-4750-9e75-bc12b2e899fb/4521ED84B9B1679A706E719423D54EF5E413DC50DDE1CF362232D7359D7E89C4/VC_redist.x86.exe".to_string(),
        sha256: "4521ed84b9b1679a706e719423d54ef5e413dc50dde1cf362232d7359d7e89c4".to_string(),
    }
});

pub static VC_REDIST_X64: Lazy<RemoteContent> = Lazy::new(|| {
    RemoteContent {
        url: "https://download.visualstudio.microsoft.com/download/pr/cd3a705f-70b6-46f7-b8e2-63e6acc5bd05/F299953673DE262FEFAD9DD19BFBE6A5725A03AE733BEBFEC856F1306F79C9F7/VC_redist.x64.exe".to_string(),
        sha256: "f299953673de262fefad9dd19bfbe6a5725a03ae733bebfec856f1306f79c9f7".to_string(),
    }
});

pub static VC_REDIST_ARM64: Lazy<RemoteContent> = Lazy::new(|| {
    RemoteContent {
        url: "https://download.visualstudio.microsoft.com/download/pr/cd3a705f-70b6-46f7-b8e2-63e6acc5bd05/D49B964641B8B2B9908A2908851A6196734B47BCC7B198C387287C438C8100B7/VC_redist.arm64.exe".to_string(),
        sha256: "d49b964641b8b2b9908a2908851a6196734b47bcc7b198c387287c438c8100b7".to_string(),
    }
});

/// Available VC++ Redistributable platforms we can add to the bundle.
#[derive(Debug, PartialEq)]
pub enum VcRedistributablePlatform {
    X86,
    X64,
    Arm64,
}

impl Display for VcRedistributablePlatform {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::X86 => "x86",
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
        })
    }
}

impl TryFrom<&str> for VcRedistributablePlatform {
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
    platform: VcRedistributablePlatform,
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
    .into_iter()
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
    _platform: VcRedistributablePlatform,
) -> Result<Vec<PathBuf>> {
    // TODO we could potentially reference these files at a URL and download them or something.
    Err(anyhow!(
        "Finding the Visual C++ Redistributable is not supported outside of Windows"
    ))
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        tugger_common::{http::download_to_path, testutil::*},
    };

    #[test]
    fn test_vcredist_download() -> Result<()> {
        let logger = get_logger()?;

        download_to_path(
            &logger,
            &VC_REDIST_X86,
            &DEFAULT_DOWNLOAD_DIR.join("vc_redist.x86.exe"),
        )?;
        download_to_path(
            &logger,
            &VC_REDIST_X64,
            &DEFAULT_DOWNLOAD_DIR.join("vc_redist.x64.exe"),
        )?;
        download_to_path(
            &logger,
            &VC_REDIST_ARM64,
            &DEFAULT_DOWNLOAD_DIR.join("vc_redist.arm64.exe"),
        )?;

        Ok(())
    }

    #[test]
    fn test_find_visual_cpp_redistributable_14() {
        let platforms = vec![
            VcRedistributablePlatform::X86,
            VcRedistributablePlatform::X64,
            VcRedistributablePlatform::Arm64,
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
    }
}
