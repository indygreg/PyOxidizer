// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! PGP signing keys. */

use {
    pgp::{
        crypto::{HashAlgorithm, SymmetricKeyAlgorithm},
        types::{CompressionAlgorithm, SecretKeyTrait},
        Deserializable, KeyType, SecretKeyParams, SecretKeyParamsBuilder, SignedPublicKey,
        SignedSecretKey,
    },
    smallvec::smallvec,
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

/// Obtain a [SecretKeyParamsBuilder] defining how to generate a signing key.
///
/// The returned builder will have defaults appropriate for Debian packaging signing keys.
///
/// The `primary_user_id` has a format like `Name <email>`. e.g. `John Smith <someone@example.com>`.
pub fn signing_secret_key_params_builder(primary_user_id: impl ToString) -> SecretKeyParamsBuilder {
    let mut key_params = SecretKeyParamsBuilder::default();
    key_params
        .key_type(KeyType::Rsa(2048))
        .preferred_symmetric_algorithms(smallvec![SymmetricKeyAlgorithm::AES256])
        .preferred_hash_algorithms(smallvec![
            HashAlgorithm::SHA2_256,
            HashAlgorithm::SHA2_384,
            HashAlgorithm::SHA2_512
        ])
        .preferred_compression_algorithms(smallvec![CompressionAlgorithm::ZLIB])
        .can_create_certificates(false)
        .can_sign(true)
        .primary_user_id(primary_user_id.to_string());

    key_params
}

/// Create a self-signed PGP key pair.
///
/// This takes [SecretKeyParams] that define the PGP key that will be generated.
/// It is recommended to use [signing_secret_key_params_builder()] to obtain these
/// params.
///
/// `key_passphrase` defines a function that will return the passphrase used to
/// lock the private key.
///
/// This returns a [SignedSecretKey] and a [SignedPublicKey] representing the
/// private-public key pair. Each key is self-signed by the just-generated private
/// key.
///
/// A self-signed PGP key pair may not be appropriate for real-world signing keys
/// on production Debian repositories. PGP best practices often entail:
///
/// * Use of sub-keys where the sub-key is used for signing and the primary key is a
///   more closely guarded secret and is only used for signing newly-created sub-keys
///   or other keys.
/// * Having a key signed by additional keys to help build a *web of trust*.
///
/// Users are highly encouraged to research PGP best practices before using the keys
/// produced by this function in a production capacity.
///
/// ```rust
/// use debian_packaging::signing_key::*;
///
/// let builder = signing_secret_key_params_builder("someone@example.com");
/// // This is where you would further customize the key parameters.
/// let params = builder.build().unwrap();
/// let (private_key, public_key) = create_self_signed_key(params, String::new).unwrap();
///
/// // You can ASCII armor the emitted key pair using the `.to_armored_*()` functions. This format
/// // is a common way to store and exchange PGP key pairs.
///
/// // Produces `-----BEGIN PGP PRIVATE KEY BLOCK----- ...`
/// let private_key_armored = private_key.to_armored_string(None).unwrap();
/// // Produces `-----BEGIN PGP PUBLIC KEY BLOCK----- ...`
/// let public_key_armored = public_key.to_armored_string(None).unwrap();
/// ```
pub fn create_self_signed_key<PW>(
    params: SecretKeyParams,
    key_passphrase: PW,
) -> pgp::errors::Result<(SignedSecretKey, SignedPublicKey)>
where
    PW: (FnOnce() -> String) + Clone,
{
    let secret_key = params.generate()?;
    let secret_key_signed = secret_key.sign(key_passphrase.clone())?;

    let public_key = secret_key_signed.public_key();
    let public_key_signed = public_key.sign(&secret_key_signed, key_passphrase)?;

    Ok((secret_key_signed, public_key_signed))
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

    #[test]
    fn key_creation() -> pgp::errors::Result<()> {
        let builder = signing_secret_key_params_builder("Me <someone@example.com>");
        let params = builder.build()?;
        let (private, public) = create_self_signed_key(params, || "passphrase".to_string())?;

        assert!(private
            .to_armored_string(None)?
            .starts_with("-----BEGIN PGP PRIVATE KEY BLOCK-----"));
        assert!(public
            .to_armored_string(None)?
            .starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----"));

        Ok(())
    }
}
