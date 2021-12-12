// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! PGP signing keys. */

use {
    pgp::{Deserializable, SignedPublicKey},
    std::io::Cursor,
    strum::EnumIter,
};

/// Release signing key for Debian 8 Jessie.
pub const DEBIAN_8_RELEASE_KEY: &str = include_str!("keys/debian-8-release.asc");

/// Archive signing key for Debian 8 Jessie.
pub const DEBIAN_8_ARCHIVE_KEY: &str = include_str!("keys/debian-8-archive.asc");

/// Security archive signing key for Debian 8 Jessie.
pub const DEBIAN_8_SECURITY_ARCHIVE_KEY: &str = include_str!("keys/debian-8-security.asc");

/// Release signing key for Debian 9 Stretch.
pub const DEBIAN_9_RELEASE_KEY: &str = include_str!("keys/debian-9-release.asc");

/// Archive signing key for Debian 9 Stretch.
pub const DEBIAN_9_ARCHIVE_KEY: &str = include_str!("keys/debian-9-archive.asc");

/// Security archive signing key for Debian 9 Stretch.
pub const DEBIAN_9_SECURITY_ARCHIVE_KEY: &str = include_str!("keys/debian-9-security.asc");

/// Release signing key for Debian 10 Buster.
pub const DEBIAN_10_RELEASE_KEY: &str = include_str!("keys/debian-10-release.asc");

/// Archive signing key for Debian 10 Buster.
pub const DEBIAN_10_ARCHIVE_KEY: &str = include_str!("keys/debian-10-archive.asc");

/// Security archive signing key for Debian 10 Buster.
pub const DEBIAN_10_SECURITY_ARCHIVE_KEY: &str = include_str!("keys/debian-10-security.asc");

/// Release signing key for Debian 11 Bullseye.
pub const DEBIAN_11_RELEASE_KEY: &str = include_str!("keys/debian-11-release.asc");

/// Archive signing key for Debian 11 Bullseye.
pub const DEBIAN_11_ARCHIVE_KEY: &str = include_str!("keys/debian-11-archive.asc");

/// Security archive signing key for Debian 11 Bullseye.
pub const DEBIAN_11_SECURITY_ARCHIVE_KEY: &str = include_str!("keys/debian-11-security.asc");

/// Defines well-known signing keys embedded within this crate.
#[derive(Clone, Copy, Debug, EnumIter)]
pub enum DistroSigningKey {
    /// Debian 9 Stretch release/stable key.
    Debian8Release,
    /// Debian 9 Stretch archive/automatic key.
    Debian8Archive,
    /// Debian 9 Stretch security archive/automatic key.
    Debian8SecurityArchive,
    /// Debian 9 Stretch release/stable key.
    Debian9Release,
    /// Debian 9 Stretch archive/automatic key.
    Debian9Archive,
    /// Debian 9 Stretch security archive/automatic key.
    Debian9SecurityArchive,
    /// Debian 10 Buster release/stable key.
    Debian10Release,
    /// Debian 10 Buster archive/automatic key.
    Debian10Archive,
    /// Debian 10 Buster security archive/automatic key.
    Debian10SecurityArchive,
    /// Debian 11 Bullseye release/stable key.
    Debian11Release,
    /// Debian 11 Bullseye archive/automatic key.
    Debian11Archive,
    /// Debian 11 Bullseye security archive/automatic key.
    Debian11SecurityArchive,
}

impl DistroSigningKey {
    /// Obtain the ASCII armored PGP public key.
    pub fn armored_public_key(&self) -> &'static str {
        match self {
            Self::Debian8Release => DEBIAN_8_RELEASE_KEY,
            Self::Debian8Archive => DEBIAN_8_ARCHIVE_KEY,
            Self::Debian8SecurityArchive => DEBIAN_8_SECURITY_ARCHIVE_KEY,
            Self::Debian9Release => DEBIAN_9_RELEASE_KEY,
            Self::Debian9Archive => DEBIAN_9_ARCHIVE_KEY,
            Self::Debian9SecurityArchive => DEBIAN_9_SECURITY_ARCHIVE_KEY,
            Self::Debian10Release => DEBIAN_10_RELEASE_KEY,
            Self::Debian10Archive => DEBIAN_10_ARCHIVE_KEY,
            Self::Debian10SecurityArchive => DEBIAN_10_SECURITY_ARCHIVE_KEY,
            Self::Debian11Release => DEBIAN_11_RELEASE_KEY,
            Self::Debian11Archive => DEBIAN_11_ARCHIVE_KEY,
            Self::Debian11SecurityArchive => DEBIAN_11_SECURITY_ARCHIVE_KEY,
        }
    }

    /// Obtain the parsed PGP public key for this variant.
    pub fn public_key(&self) -> SignedPublicKey {
        SignedPublicKey::from_armor_single(Cursor::new(self.armored_public_key().as_bytes()))
            .expect("built-in signing keys should parse")
            .0
    }
}

#[cfg(test)]
mod test {
    use {super::*, strum::IntoEnumIterator};

    #[test]
    fn all_distro_signing_keys() {
        for key in DistroSigningKey::iter() {
            key.public_key();
        }
    }
}
