// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Context, Result},
    duct::cmd,
    once_cell::sync::Lazy,
    semver::Version,
    serde::Deserialize,
    std::{
        collections::HashMap,
        path::{Path, PathBuf},
    },
};

/// Default install path for the Xcode command line tools.
pub static COMMAND_LINE_TOOLS_DEFAULT_PATH: Lazy<PathBuf> =
    Lazy::new(|| PathBuf::from("/Library/Developer/CommandLineTools"));

/// Default path to Xcode application.
pub static XCODE_APP_DEFAULT_PATH: Lazy<PathBuf> =
    Lazy::new(|| PathBuf::from("/Applications/Xcode.app"));

/// Relative path under Xcode.app directories defining a `Developer` directory.
///
/// This directory contains platforms, toolchains, etc.
pub static XCODE_APP_RELATIVE_PATH_DEVELOPER: Lazy<PathBuf> =
    Lazy::new(|| PathBuf::from("Contents/Developer"));

/// Represents the DefaultProperties key in a SDKSettings.json file.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct SdkSettingsJsonDefaultProperties {
    platform_name: String,
}

/// Represents a SupportedTargets value in a SDKSettings.json file.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AppleSdkSupportedTarget {
    pub archs: Vec<String>,
    pub default_deployment_target: String,
    pub default_variant: Option<String>,
    pub deployment_target_setting_name: Option<String>,
    pub minimum_deployment_target: String,
    pub platform_family_name: Option<String>,
    pub valid_deployment_targets: Vec<String>,
}

/// Used for deserializing a SDKSettings.json file in an SDK directory.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SdkSettingsJson {
    canonical_name: String,
    default_deployment_target: String,
    default_properties: SdkSettingsJsonDefaultProperties,
    default_variant: Option<String>,
    display_name: String,
    maximum_deployment_target: String,
    minimal_display_name: String,
    supported_targets: HashMap<String, AppleSdkSupportedTarget>,
    version: String,
}

/// Describes an Apple SDK on the filesystem.
#[derive(Clone, Debug)]
pub struct AppleSdk {
    /// Root directory of the SDK.
    pub path: PathBuf,

    /// Whether the root directory is a symlink to another path.
    pub is_symlink: bool,

    /// The name of the platform
    pub platform_name: String,

    /// The canonical name of the SDK. e.g. `macosx11.1`.
    pub name: String,

    /// Version of the default deployment target for this SDK.
    pub default_deployment_target: String,

    /// Name of default settings variant for this SDK.
    pub default_variant: Option<String>,

    /// Human friendly name of this SDK.
    pub display_name: String,

    /// Maximum deployment target version this SDK supports.
    pub maximum_deployment_target: String,

    /// Human friendly value for name (probably just version string).
    pub minimal_display_name: String,

    /// Describes named target configurations this SDK supports.
    pub supported_targets: HashMap<String, AppleSdkSupportedTarget>,

    /// Version of this SDK. e.g. `11.1`.
    pub version: String,
}

impl AppleSdk {
    /// Attempt to resolve an SDK from a path to the SDK root directory.
    pub fn from_directory(path: &Path) -> Result<Self> {
        // Need to call symlink_metadata so symlinks aren't followed.
        let metadata =
            std::fs::symlink_metadata(path).context("reading directory entry metadata")?;

        let is_symlink = metadata.file_type().is_symlink();

        let settings_path = path.join("SDKSettings.json");

        let json_data = std::fs::read(&settings_path)
            .with_context(|| format!("reading {}", settings_path.display()))?;

        let settings_json: SdkSettingsJson = serde_json::from_slice(&json_data)
            .with_context(|| format!("parsing {}", settings_path.display()))?;

        Ok(Self::from_json(
            path.to_path_buf(),
            is_symlink,
            settings_json,
        ))
    }

    /// Attempt to create a new instance from deserialized JSON.
    fn from_json(path: PathBuf, is_symlink: bool, value: SdkSettingsJson) -> Self {
        Self {
            path,
            is_symlink,
            platform_name: value.default_properties.platform_name,
            name: value.canonical_name,
            default_deployment_target: value.default_deployment_target,
            default_variant: value.default_variant,
            display_name: value.display_name,
            maximum_deployment_target: value.maximum_deployment_target,
            minimal_display_name: value.minimal_display_name,
            supported_targets: value.supported_targets,
            version: value.version,
        }
    }

    /// Convert the version string to a `semver::Version`.
    pub fn version_as_semver(&self) -> Result<Version> {
        match self.version.split('.').count() {
            2 => Ok(Version::parse(&format!("{}.0", self.version))?),
            3 => Ok(Version::parse(&self.version)?),
            _ => Err(anyhow!(
                "version string {} is not of form X.Y or X.Y.Z",
                self.version
            )),
        }
    }
}

/// Obtain the current developer directory where SDKs and tools are installed.
///
/// This returns the `DEVELOPER_DIR` environment variable if found or
/// uses the `xcode-select` logic for locating the developer directory if not.
/// Failure the locate a directory results in `Err`.
///
/// The returned path is not verified to exist.
pub fn default_developer_directory() -> Result<PathBuf> {
    // DEVELOPER_DIR environment variable overrides any settings.
    if let Ok(env) = std::env::var("DEVELOPER_DIR") {
        Ok(PathBuf::from(env))
    } else {
        // We use xcode-select to find the directory. But this probably
        // just reads from a plist or something. We could potentially
        // reimplement this logic in pure Rust...
        let res = cmd("xcode-select", &["--print-path"])
            .stderr_null()
            .read()
            .context("running xcode-select")?;

        Ok(PathBuf::from(res))
    }
}

/// Obtain the path to the `Developer` directory in the default Xcode app.
///
/// Returns `Some` if Xcode is installed in its default location and has
/// a `Developer` directory or `None` if not.
pub fn default_xcode_developer_directory() -> Option<PathBuf> {
    let path = XCODE_APP_DEFAULT_PATH.join(XCODE_APP_RELATIVE_PATH_DEVELOPER.as_path());

    if path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Attempt to resolve all available Xcode applications in an `Applications` directory.
///
/// This function is a convenience method for iterating a directory
/// and filtering for `Xcode*.app` entries.
///
/// No guarantee is made about the return order or whether the
/// directory constitutes a working Xcode application.
pub fn find_xcode_apps(applications_dir: &Path) -> Result<Vec<PathBuf>> {
    let dir = match std::fs::read_dir(&applications_dir) {
        Ok(v) => Ok(v),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Ok(vec![]);
            } else {
                Err(anyhow!("error reading directory: {}", e))
            }
        }
    }?;

    Ok(dir
        .into_iter()
        .map(|entry| {
            let entry = entry.context("reading directory entry")?;

            let name = entry.file_name();
            let file_name = name.to_string_lossy();

            if file_name.starts_with("Xcode") && file_name.ends_with(".app") {
                Ok(Some(entry.path()))
            } else {
                Ok(None)
            }
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|x| x)
        .collect::<Vec<_>>())
}

/// Find all system installed Xcode applications.
///
/// This is a convenience method for `find_xcode_apps()` looking under `/Applications`.
/// This location is typically where Xcode is installed.
pub fn find_system_xcode_applications() -> Result<Vec<PathBuf>> {
    find_xcode_apps(&PathBuf::from("/Applications"))
}

/// Finds all `Developer` directories for installed Xcode applications for system application installs.
///
/// This is a convenience method for `find_system_xcode_applications()` plus
/// resolving the `Developer` directory and filtering on missing items.
///
/// It will return all available `Developer` directories for all Xcode installs
/// under `/Applications`.
pub fn find_system_xcode_developer_directories() -> Result<Vec<PathBuf>> {
    Ok(find_system_xcode_applications()
        .context("finding system xcode applications")?
        .into_iter()
        .filter_map(|p| {
            let developer_path = p.join(XCODE_APP_RELATIVE_PATH_DEVELOPER.as_path());

            if developer_path.exists() {
                Some(developer_path)
            } else {
                None
            }
        })
        .collect::<Vec<_>>())
}

/// Attempt to derive the name of a platform from a directory path.
///
/// Returns `Some(platform)` if the directory represents a platform or
/// `None` otherwise.
fn platform_from_path(path: &Path) -> Option<String> {
    if let Some(file_name) = path.file_name() {
        if let Some(name) = file_name.to_str() {
            let parts = name.splitn(2, '.').collect::<Vec<_>>();

            if parts.len() == 2 && parts[1] == "platform" {
                return Some(parts[0].to_string());
            }
        }
    }

    None
}

/// Find "platforms" given a developer directory.
///
/// Platforms are effectively targets that can be built for.
///
/// Platforms are defined by the presence of a `Platforms` directory under
/// the developer directory. This directory layout is only recognized
/// for modern Xcode layouts.
///
/// Returns a vector of (platform, path) tuples denoting the platform
/// name and its base directory.
pub fn find_developer_platforms(developer_dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let platforms_path = developer_dir.join("Platforms");

    let dir = match std::fs::read_dir(&platforms_path) {
        Ok(v) => Ok(v),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Ok(vec![]);
            } else {
                Err(anyhow!("error reading directory: {}", e))
            }
        }
    }?;

    let mut res = vec![];

    for entry in dir {
        let entry = entry.context("reading directory entry")?;

        if let Some(platform) = platform_from_path(&entry.path()) {
            res.push((platform, entry.path()));
        }
    }

    Ok(res)
}

/// Finds SDKs in a specified directory.
///
///
/// Directory entries are often symlinks pointing to other directories.
/// SDKs are annotated with an `is_symlink` field to denote when this is
/// the case. Callers may want to filter out symlinked SDKs to avoid
/// duplicates.
pub fn find_sdks_in_directory(root: &Path) -> Result<Vec<AppleSdk>> {
    let dir = match std::fs::read_dir(&root) {
        Ok(v) => Ok(v),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Ok(vec![]);
            } else {
                Err(anyhow!("error reading directory: {}", e))
            }
        }
    }?;

    let mut res = vec![];

    for entry in dir {
        let entry = entry.context("reading directory entry")?;

        let settings_path = entry.path().join("SDKSettings.json");

        if !settings_path.exists() {
            continue;
        }

        res.push(AppleSdk::from_directory(&entry.path())?);
    }

    Ok(res)
}

/// Finds SDKs in a platform directory.
///
/// This function is a simple wrapper around `find_sdks_in_directory()`
/// looking under the `Developer/SDKs` directory, which is the path under
/// platform directories containing SDKs.
pub fn find_sdks_in_platform(platform_dir: &Path) -> Result<Vec<AppleSdk>> {
    let sdks_path = platform_dir.join("Developer").join("SDKs");

    find_sdks_in_directory(&sdks_path)
}

/// Locate SDKs given the path to a developer directory.
///
/// This is effectively a convenience method for calling
/// `find_developer_platforms()` + `find_sdks_in_platform()` and chaining the
/// results.
pub fn find_developer_sdks(developer_dir: &Path) -> Result<Vec<AppleSdk>> {
    Ok(find_developer_platforms(developer_dir)
        .context("finding developer platforms")?
        .into_iter()
        .map(|(_, platform_path)| {
            Ok(find_sdks_in_platform(&platform_path)
                .with_context(|| format!("finding SDKs in {}", platform_path.display()))?
                .into_iter())
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>())
}

/// Discover SDKs in the default developer directory.
pub fn find_default_developer_sdks() -> Result<Vec<AppleSdk>> {
    let developer_dir =
        default_developer_directory().context("locating default developer directory")?;

    find_developer_sdks(&developer_dir).context("finding SDKs in developer directory")
}

/// Locate SDKs installed as part of the Xcode Command Line Tools.
///
/// This is a convenience method for looking for SDKs in the `SDKs` directory
/// under the default install path for the Xcode Command Line Tools.
///
/// Returns `Ok(None)` if the Xcode Command Line Tools are not present in
/// this directory or doesn't have an `SDKs` directory.
pub fn find_command_line_tools_sdks() -> Result<Option<Vec<AppleSdk>>> {
    let sdk_path = COMMAND_LINE_TOOLS_DEFAULT_PATH.join("SDKs");

    if !sdk_path.exists() {
        Ok(None)
    } else {
        Ok(Some(find_sdks_in_directory(&sdk_path)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_system_xcode_applications() -> Result<()> {
        let res = find_system_xcode_applications()?;

        if XCODE_APP_DEFAULT_PATH.exists() {
            assert!(!res.is_empty());
        }

        Ok(())
    }

    #[test]
    fn test_find_system_xcode_developer_directories() -> Result<()> {
        let res = find_system_xcode_developer_directories()?;

        if XCODE_APP_DEFAULT_PATH.exists() {
            assert!(!res.is_empty());
        }

        Ok(())
    }

    #[test]
    fn test_find_default_sdks() -> Result<()> {
        if let Ok(developer_dir) = default_developer_directory() {
            assert!(!find_developer_sdks(&developer_dir)?.is_empty());
            assert!(!find_default_developer_sdks()?.is_empty());
        }

        Ok(())
    }

    #[test]
    fn test_find_command_line_tools_sdks() -> Result<()> {
        let sdk_path = COMMAND_LINE_TOOLS_DEFAULT_PATH.join("SDKs");

        let res = find_command_line_tools_sdks()?;

        if sdk_path.exists() {
            assert!(res.is_some());
            assert!(!res.unwrap().is_empty());
        } else {
            assert!(res.is_none());
        }

        Ok(())
    }

    /// Verifies various discovery operations on a macOS GitHub Actions runner.
    ///
    /// This assumes we're using GitHub's official macOS runners.
    #[cfg(target_os = "macos")]
    #[test]
    fn test_github_actions() -> Result<()> {
        if std::env::var("GITHUB_ACTIONS").is_err() {
            return Ok(());
        }

        assert_eq!(
            default_xcode_developer_directory(),
            Some(PathBuf::from("/Applications/Xcode.app/Contents/Developer"))
        );
        assert!(COMMAND_LINE_TOOLS_DEFAULT_PATH.exists());

        // GitHub Actions runners have multiple Xcode applications installed.
        assert!(find_system_xcode_applications()?.len() > 5);

        // We should be able to resolve developer directories for all system Xcode
        // applications.
        assert_eq!(
            find_system_xcode_applications()?.len(),
            find_system_xcode_developer_directories()?.len()
        );

        // We should be able to resolve SDKs for all system Xcode applications.
        for path in find_system_xcode_developer_directories()? {
            find_developer_sdks(&path)?;
        }

        Ok(())
    }
}
