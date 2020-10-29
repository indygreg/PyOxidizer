// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod bundle_builder;
mod common;
mod installer_builder;
mod simple_msi_builder;
mod wxs_builder;

pub use bundle_builder::WiXBundleInstallerBuilder;
pub use common::{run_candle, run_light, target_triple_to_wix_arch, write_file_manifest_to_wix};
pub use installer_builder::WiXInstallerBuilder;
pub use simple_msi_builder::WiXSimpleMSIBuilder;
pub use wxs_builder::WxsBuilder;

use {
    crate::{
        http::{download_to_path, RemoteContent},
        zipfile::extract_zip,
    },
    anyhow::{Context, Result},
    handlebars::Handlebars,
    lazy_static::lazy_static,
    slog::warn,
    std::{
        io::Write,
        path::{Path, PathBuf},
    },
    xml::{
        common::XmlVersion,
        writer::{EmitterConfig, EventWriter, XmlEvent},
    },
};

lazy_static! {
    static ref WIX_TOOLSET: RemoteContent = RemoteContent {
        url: "https://github.com/wixtoolset/wix3/releases/download/wix3112rtm/wix311-binaries.zip"
            .to_string(),
        sha256: "2c1888d5d1dba377fc7fa14444cf556963747ff9a0a289a3599cf09da03b9e2e".to_string(),
    };

    // Latest versions of the VC++ Redistributable can be found at
    // https://support.microsoft.com/en-us/help/2977003/the-latest-supported-visual-c-downloads.
    // The download URL will redirect to a deterministic artifact, which is what we
    // record here.

    static ref VC_REDIST_X86: RemoteContent = RemoteContent {
        url: "https://download.visualstudio.microsoft.com/download/pr/48431a06-59c5-4b63-a102-20b66a521863/CAA38FD474164A38AB47AC1755C8CCCA5CCFACFA9A874F62609E6439924E87EC/VC_redist.x86.exe".to_string(),
        sha256: "caa38fd474164a38ab47ac1755c8ccca5ccfacfa9a874f62609e6439924e87ec".to_string(),
    };

    static ref VC_REDIST_X64: RemoteContent = RemoteContent {
        url: "https://download.visualstudio.microsoft.com/download/pr/48431a06-59c5-4b63-a102-20b66a521863/4B5890EB1AEFDF8DFA3234B5032147EB90F050C5758A80901B201AE969780107/VC_redist.x64.exe".to_string(),
        sha256: "4b5890eb1aefdf8dfa3234b5032147eb90f050c5758a80901b201ae969780107".to_string(),
    };

    static ref VC_REDIST_ARM64: RemoteContent = RemoteContent {
        url: "https://download.visualstudio.microsoft.com/download/pr/48431a06-59c5-4b63-a102-20b66a521863/A950A1C9DB37E2F784ABA98D484A4E0F77E58ED7CB57727672F9DC321015469E/VC_redist.arm64.exe".to_string(),
        sha256: "a950a1c9db37e2f784aba98d484a4e0f77e58ed7cb57727672f9dc321015469e".to_string(),
    };

    static ref HANDLEBARS: Handlebars<'static> = {
        let mut handlebars = Handlebars::new();

        handlebars
            .register_template_string("bundle.wxs", include_str!("templates/bundle.wxs"))
            .unwrap();

        handlebars
    };
}

fn extract_wix<P: AsRef<Path>>(logger: &slog::Logger, dest_dir: P) -> Result<PathBuf> {
    let dest_dir = dest_dir.as_ref();

    if !dest_dir.exists() {
        std::fs::create_dir_all(dest_dir)
            .with_context(|| format!("creating {}", dest_dir.display()))?;
    }

    let zip_path = dest_dir.join(format!("wix-toolset.{}.zip", &WIX_TOOLSET.sha256[0..16]));
    let extract_path = dest_dir.join(format!("wix-toolset.{}", &WIX_TOOLSET.sha256[0..16]));

    if !extract_path.exists() {
        download_to_path(logger, &WIX_TOOLSET, &zip_path)
            .with_context(|| format!("downloading to {}", zip_path.display()))?;
        let fh = std::fs::File::open(&zip_path)?;
        let cursor = std::io::BufReader::new(fh);
        warn!(logger, "extracting WiX...");
        extract_zip(cursor, &extract_path)
            .with_context(|| format!("extracting zip to {}", extract_path.display()))?;
    }

    Ok(extract_path)
}

#[cfg(test)]
mod tests {
    use {super::*, crate::testutil::*};

    #[test]
    fn test_wix_download() -> Result<()> {
        let logger = get_logger()?;

        extract_wix(&logger, DEFAULT_DOWNLOAD_DIR.as_path())?;

        Ok(())
    }

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
}
