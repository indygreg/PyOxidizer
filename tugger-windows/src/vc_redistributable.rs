// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    once_cell::sync::Lazy,
    std::{
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
        name: "VC_REDIST_X86".to_string(),
        url: "https://download.visualstudio.microsoft.com/download/pr/888b4c07-c602-499a-9efb-411188496ce7/F3A86393234099BEDD558FD35AB538A6E4D9D4F99AD5ADFA13F603D4FF8A42DC/VC_redist.x86.exe".to_string(),
        sha256: "f3a86393234099bedd558fd35ab538a6e4d9d4f99ad5adfa13f603d4ff8a42dc".to_string(),
    }
});

pub static VC_REDIST_X64: Lazy<RemoteContent> = Lazy::new(|| {
    RemoteContent {
        name: "VC_REDIST_X64".to_string(),
        url: "https://download.visualstudio.microsoft.com/download/pr/36e45907-8554-4390-ba70-9f6306924167/97CC5066EB3C7246CF89B735AE0F5A5304A7EE33DC087D65D9DFF3A1A73FE803/VC_redist.x64.exe".to_string(),
        sha256: "97cc5066eb3c7246cf89b735ae0f5a5304a7ee33dc087d65d9dff3a1a73fe803".to_string(),
    }
});

pub static VC_REDIST_ARM64: Lazy<RemoteContent> = Lazy::new(|| {
    RemoteContent {
        name: "VC_REDIST_ARM64".to_string(),
        url: "https://download.visualstudio.microsoft.com/download/pr/888b4c07-c602-499a-9efb-411188496ce7/B76EF09CD8B114148EADDDFC6846EF178E6B7797F590191E22CEE29A20B51692/VC_redist.arm64.exe".to_string(),
        sha256: "b76ef09cd8b114148eadddfc6846ef178e6b7797f590191e22cee29a20b51692".to_string(),
    }
});

/// Available VC++ Redistributable platforms we can add to the bundle.
#[derive(Debug, PartialEq, Eq)]
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
    type Error = anyhow::Error;

    fn try_from(value: &str) -> anyhow::Result<Self, Self::Error> {
        match value {
            "x86" => Ok(Self::X86),
            "x64" => Ok(Self::X64),
            "arm64" => Ok(Self::Arm64),
            _ => Err(anyhow!(
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
            .or_insert_with(Vec::new)
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
        download_to_path(
            &VC_REDIST_X86,
            &DEFAULT_DOWNLOAD_DIR.join("vc_redist.x86.exe"),
        )?;
        download_to_path(
            &VC_REDIST_X64,
            &DEFAULT_DOWNLOAD_DIR.join("vc_redist.x64.exe"),
        )?;
        download_to_path(
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
