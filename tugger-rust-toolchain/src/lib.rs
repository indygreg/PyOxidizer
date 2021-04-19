// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Obtain and interact with Rust toolchains.
//!
//! This module effectively reimplements the Rust toolchain discovery
//! and download features of `rustup` to facilitate automatic Rust toolchain
//! install. This enables people without Rust on their machines to easily
//! use PyOxidizer.

pub mod manifest;
pub mod tar;

use {
    crate::{manifest::Manifest, tar::PackageArchive},
    anyhow::{anyhow, Context, Result},
    once_cell::sync::Lazy,
    pgp::{Deserializable, SignedPublicKey, StandaloneSignature},
    sha2::Digest,
    slog::warn,
    std::{
        io::{Cursor, Read},
        path::{Path, PathBuf},
    },
    tugger_common::http::{download_and_verify, download_to_path, get_http_client},
};

const URL_PREFIX: &str = "https://static.rust-lang.org/dist/";

static GPG_SIGNING_KEY: Lazy<SignedPublicKey> = Lazy::new(|| {
    pgp::SignedPublicKey::from_armor_single(Cursor::new(&include_bytes!("signing-key.asc")[..]))
        .unwrap()
        .0
});

/// Fetch, verify, and parse a Rust toolchain manifest for a named channel.
///
/// Returns the verified and parsed manifest.
pub fn fetch_channel_manifest(logger: &slog::Logger, channel: &str) -> Result<Manifest> {
    let manifest_url = format!("{}channel-rust-{}.toml", URL_PREFIX, channel);
    let signature_url = format!("{}.asc", manifest_url);
    let sha256_url = format!("{}.sha256", manifest_url);

    let client = get_http_client()?;

    warn!(logger, "fetching {}", sha256_url);
    let mut response = client.get(&sha256_url).send()?;
    let mut sha256_data = vec![];
    response.read_to_end(&mut sha256_data)?;

    let sha256_manifest = String::from_utf8(sha256_data)?;
    let manifest_digest_wanted = sha256_manifest
        .split(' ')
        .next()
        .ok_or_else(|| anyhow!("failed parsing SHA-256 manifest"))?
        .to_string();

    warn!(logger, "fetching {}", manifest_url);
    let mut response = client.get(&manifest_url).send()?;
    let mut manifest_data = vec![];
    response.read_to_end(&mut manifest_data)?;

    warn!(logger, "fetching {}", signature_url);
    let mut response = client.get(&signature_url).send()?;
    let mut signature_data = vec![];
    response.read_to_end(&mut signature_data)?;

    let mut hasher = sha2::Sha256::new();
    hasher.update(&manifest_data);

    let manifest_digest_got = hex::encode(hasher.finalize().to_vec());

    if manifest_digest_got != manifest_digest_wanted {
        return Err(anyhow!(
            "digest mismatch on {}; wanted {}, got {}",
            manifest_url,
            manifest_digest_wanted,
            manifest_digest_got
        ));
    }

    warn!(logger, "verified SHA-256 digest for {}", manifest_url);

    let (signatures, _) = StandaloneSignature::from_armor_many(Cursor::new(&signature_data))
        .with_context(|| format!("parsing {} armored signature data", signature_url))?;

    for signature in signatures {
        let signature = signature.context("obtaining pgp signature")?;

        signature
            .verify(&*GPG_SIGNING_KEY, &manifest_data)
            .context("verifying pgp signature of manifest")?;
        warn!(logger, "verified PGP signature for {}", manifest_url);
    }

    let manifest = Manifest::from_toml_bytes(&manifest_data).context("parsing manifest TOML")?;

    Ok(manifest)
}

/// Materialize a Rust package in a directory.
///
/// Given a parsed manifest, this will attempt to locate the package entry,
/// download and verify it, and extract it to the directory specified.
///
/// `download_cache_dir` specifies an optional path where to cache downloaded
/// archives.
pub fn materialize_rust_package(
    logger: &slog::Logger,
    manifest: &Manifest,
    package: &str,
    target_triple: &str,
    install_dir: &Path,
    download_cache_dir: Option<&Path>,
) -> Result<()> {
    let (version, target) = manifest
        .find_package(package, target_triple)
        .ok_or_else(|| {
            anyhow!(
                "package {} not available for target triple {}",
                package,
                target_triple
            )
        })?;

    warn!(
        logger,
        "found Rust package {} version {} for {}", package, version, target_triple
    );

    let (compression_format, remote_content) = target.download_info().ok_or_else(|| {
        anyhow!(
            "package {} for target {} is not available",
            package,
            target_triple
        )
    })?;

    let tar_data = if let Some(download_dir) = download_cache_dir {
        let dest_path = download_dir.join(&remote_content.sha256);

        download_to_path(logger, &remote_content, &dest_path)
            .context("downloading file to cache directory")?;

        std::fs::read(&dest_path).context("reading downloaded file")?
    } else {
        download_and_verify(logger, &remote_content)?
    };

    let archive =
        PackageArchive::new(compression_format, tar_data).context("obtaining PackageArchive")?;

    archive.install(install_dir).context("installing")?;

    Ok(())
}

/// Represents an installed toolchain on the filesystem.
#[derive(Clone, Debug)]
pub struct InstalledToolchain {
    /// Root directory of this toolchain.
    pub path: PathBuf,

    /// Path to executable binaries in this toolchain.
    ///
    /// Suitable for inclusion on `PATH`.
    pub bin_path: PathBuf,

    /// Path to `rustc` executable.
    pub rustc_path: PathBuf,

    /// Path to `cargo` executable.
    pub cargo_path: PathBuf,
}

/// Install a functional Rust toolchain capable of running on and building for a target triple.
///
/// This is a convenience method for fetching the packages that compose a minimal
/// Rust installation capable of compiling.
///
/// `host_triple` denotes the host triple of the toolchain to fetch.
/// `extra_target_triples` denotes extra triples for targets we are building for.
pub fn install_rust_toolchain(
    logger: &slog::Logger,
    toolchain: &str,
    host_triple: &str,
    extra_target_triples: &[&str],
    install_root_dir: &Path,
    download_cache_dir: Option<&Path>,
) -> Result<InstalledToolchain> {
    let manifest = fetch_channel_manifest(logger, toolchain).context("fetching manifest")?;

    // The actual install directory is composed of the toolchain name and the
    // host triple.
    let install_dir = install_root_dir.join(format!("{}-{}", toolchain, host_triple));

    for component in &["rustc", "cargo", "rust-std"] {
        materialize_rust_package(
            logger,
            &manifest,
            component,
            host_triple,
            &install_dir,
            download_cache_dir,
        )?;
    }

    for triple in extra_target_triples {
        if *triple != host_triple {
            materialize_rust_package(
                logger,
                &manifest,
                "rust-std",
                triple,
                &install_dir,
                download_cache_dir,
            )?;
        }
    }

    let exe_suffix = if host_triple.contains("-windows-") {
        ".exe"
    } else {
        ""
    };

    Ok(InstalledToolchain {
        path: install_dir.clone(),
        bin_path: install_dir.join("bin"),
        rustc_path: install_dir.join("bin").join(format!("rustc{}", exe_suffix)),
        cargo_path: install_dir.join("bin").join(format!("cargo{}", exe_suffix)),
    })
}

#[cfg(test)]
mod tests {
    use {super::*, tugger_common::testutil::get_logger};

    const TEST_TRIPLES: &[&str; 3] = &[
        "x86_64-apple-darwin",
        "x86_64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
    ];

    #[test]
    fn fetch_stable() -> Result<()> {
        let logger = get_logger()?;

        fetch_channel_manifest(&logger, "stable")?;

        Ok(())
    }

    #[test]
    fn fetch_common_packages() -> Result<()> {
        let logger = get_logger()?;

        for target_triple in TEST_TRIPLES {
            let temp_dir = tempfile::Builder::new()
                .prefix("tugger-rust-toolchain-test")
                .tempdir()?;

            let toolchain = install_rust_toolchain(
                &logger,
                "stable",
                target_triple,
                &[],
                temp_dir.path(),
                None,
            )?;

            assert_eq!(
                toolchain.path,
                temp_dir.path().join(format!("stable-{}", target_triple))
            );
        }

        Ok(())
    }
}
