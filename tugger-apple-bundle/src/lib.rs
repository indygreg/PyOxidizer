// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod directory_bundle;
pub use directory_bundle::*;
mod macos_application_bundle;
pub use macos_application_bundle::*;

/// Denotes the type of a bundle.
pub enum BundlePackageType {
    /// Application bundle.
    App,
    /// Framework bundle.
    Framework,
    /// Generic bundle.
    Bundle,
}

impl ToString for BundlePackageType {
    fn to_string(&self) -> String {
        match self {
            Self::App => "APPL",
            Self::Framework => "FMWK",
            Self::Bundle => "BNDL",
        }
        .to_string()
    }
}
