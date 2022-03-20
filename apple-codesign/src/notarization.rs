// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Apple notarization functionality. */

use std::path::PathBuf;

pub const TRANSPORTER_PATH_ENV_VARIABLE: &str = "APPLE_CODESIGN_TRANSPORTER_EXE";

/// Where Apple installs transporter by default on Linux and macOS.
const TRANSPORTER_DEFAULT_PATH_POSIX: &str = "/usr/local/itms/bin/iTMSTransporter";

/// Find the transporter executable to use for notarization.
///
/// See https://help.apple.com/itc/transporteruserguide/#/apdAbeb95d60 for instructions
/// on installing Transporter and where default installs are often located.
pub fn find_transporter_exe() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(TRANSPORTER_PATH_ENV_VARIABLE) {
        Some(PathBuf::from(path))
    } else if let Ok(path) = which::which("iTMSTransporter") {
        Some(path)
    } else {
        let candidate = PathBuf::from(TRANSPORTER_DEFAULT_PATH_POSIX);

        if candidate.exists() {
            return Some(candidate);
        }

        for env in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(path) = std::env::var_os(env) {
                let candidate = PathBuf::from(path).join("itms").join("iTMSTransporter.cmd");

                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        None
    }
}
