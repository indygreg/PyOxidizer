// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use lazy_static::lazy_static;
use std::path::{Path, PathBuf};

/// terminfo directories for Debian based distributions.
///
/// Search for `--with-terminfo-dirs` at
/// https://salsa.debian.org/debian/ncurses/blob/master/debian/rules to find
/// the source of truth for this.
const TERMINFO_DIRS_DEBIAN: &str = "/etc/terminfo:/lib/terminfo:/usr/share/terminfo";

/// terminfo directories for RedHat based distributions.
///
/// CentOS compiled with
/// `--with-terminfo-dirs=%{_sysconfdir}/terminfo:%{_datadir}/terminfo`.
const TERMINFO_DIRS_REDHAT: &str = "/etc/terminfo:/usr/share/terminfo";

/// terminfo directories for macOS.
const TERMINFO_DIRS_MACOS: &str = "/usr/share/terminfo";

lazy_static! {
    static ref TERMINFO_DIRS_COMMON: Vec<PathBuf> = {
        vec![
            PathBuf::from("/usr/local/etc/terminfo"),
            PathBuf::from("/usr/local/lib/terminfo"),
            PathBuf::from("/usr/local/share/terminfo"),
            PathBuf::from("/etc/terminfo"),
            PathBuf::from("/usr/lib/terminfo"),
            PathBuf::from("/lib/terminfo"),
            PathBuf::from("/usr/share/terminfo"),
        ]
    };
}

#[derive(Clone)]
enum OsVariant {
    Linux,
    MacOs,
    Windows,
    Other,
}

enum LinuxDistroVariant {
    Debian,
    RedHat,
    Unknown,
}

lazy_static! {
    static ref TARGET_OS: OsVariant = {
        if cfg!(target_os = "linux") {
            OsVariant::Linux
        } else if cfg!(target_os = "macos") {
            OsVariant::MacOs
        } else if cfg!(target_os = "windows") {
            OsVariant::Windows
        } else {
            OsVariant::Other
        }
    };
}

struct OsInfo {
    os: OsVariant,
    linux_distro: Option<LinuxDistroVariant>,
}

fn resolve_linux_distro() -> LinuxDistroVariant {
    // Attempt to resolve the Linux distro by parsing /etc files.
    let os_release = Path::new("/etc/os-release");

    if let Ok(data) = std::fs::read_to_string(os_release) {
        for line in data.split("\n") {
            if line.starts_with("ID_LIKE=") {
                if line.contains("debian") {
                    return LinuxDistroVariant::Debian;
                } else if line.contains("rhel") || line.contains("fedora") {
                    return LinuxDistroVariant::RedHat;
                }
            } else if line.starts_with("ID=") {
                if line.contains("fedora") {
                    return LinuxDistroVariant::RedHat;
                }
            }
        }
    }

    LinuxDistroVariant::Unknown
}

fn resolve_os_info() -> OsInfo {
    let os = TARGET_OS.clone();
    let linux_distro = match os {
        OsVariant::Linux => Some(resolve_linux_distro()),
        _ => None,
    };

    OsInfo { os, linux_distro }
}

/// Attempt to resolve the value for the `TERMINFO_DIRS` environment variable.
///
/// Returns Some() value that `TERMINFO_DIRS` should be set to or None if
/// no environment variable should be set.
pub fn resolve_terminfo_dirs() -> Option<String> {
    // Always respect an environment variable, if present.
    if std::env::var("TERMINFO_DIRS").is_ok() {
        return None;
    }

    let os_info = resolve_os_info();

    match os_info.os {
        OsVariant::Linux => match os_info.linux_distro.unwrap() {
            // TODO we could stat() the well-known paths ourselves and omit
            // paths that don't exist. This /might/ save some syscalls, since
            // ncurses doesn't appear to be the most frugal w.r.t. filesystem
            // requests.
            LinuxDistroVariant::Debian => Some(TERMINFO_DIRS_DEBIAN.to_string()),
            LinuxDistroVariant::RedHat => Some(TERMINFO_DIRS_REDHAT.to_string()),
            LinuxDistroVariant::Unknown => {
                // We don't know this Linux variant. Look for common terminfo
                // database directories and use paths that are found.
                let paths = TERMINFO_DIRS_COMMON
                    .iter()
                    .filter_map(|p| {
                        if p.exists() {
                            Some(p.display().to_string())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<String>>()
                    .join(":");

                Some(paths)
            }
        },
        OsVariant::MacOs => Some(TERMINFO_DIRS_MACOS.to_string()),
        // Windows doesn't use the terminfo database.
        OsVariant::Windows => None,
        OsVariant::Other => None,
    }
}
