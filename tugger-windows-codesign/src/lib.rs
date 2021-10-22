// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Code signing on Windows. */

mod signing;
pub use signing::*;
mod signtool;
pub use signtool::*;

/// Defines a specific Windows certificate system store.
///
/// See https://docs.microsoft.com/en-us/windows/win32/seccrypto/system-store-locations
/// for meanings.
#[derive(Clone, Copy, Debug)]
pub enum SystemStore {
    My,
    Root,
    Trust,
    Ca,
    UserDs,
}

impl Default for SystemStore {
    fn default() -> Self {
        Self::My
    }
}

impl AsRef<str> for SystemStore {
    fn as_ref(&self) -> &str {
        match self {
            Self::My => "MY",
            Self::Root => "Root",
            Self::Trust => "Trust",
            Self::Ca => "CA",
            Self::UserDs => "UserDS",
        }
    }
}

impl TryFrom<&str> for SystemStore {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "my" => Ok(Self::My),
            "root" => Ok(Self::Root),
            "trust" => Ok(Self::Trust),
            "ca" => Ok(Self::Ca),
            "userds" => Ok(Self::UserDs),
            _ => Err(format!("{} is not a valid system store value", value)),
        }
    }
}
