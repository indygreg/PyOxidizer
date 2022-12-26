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
    crate::{
        manifest::Manifest,
        tar::{read_installs_manifest, PackageArchive},
    },
    anyhow::{anyhow, Context, Result},
    fs2::FileExt,
    log::warn,
    once_cell::sync::Lazy,
    pgp::{Deserializable, SignedPublicKey, StandaloneSignature},
    sha2::Digest,
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
pub fn fetch_channel_manifest(channel: &str) -> Result<Manifest> {
    let manifest_url = format!("{}channel-rust-{}.toml", URL_PREFIX, channel);
    let signature_url = format!("{}.asc", manifest_url);
    let sha256_url = format!("{}.sha256", manifest_url);

    let client = get_http_client()?;

    warn!("fetching {}", sha256_url);
    let mut response = client.get(&sha256_url).send()?;
    let mut sha256_data = vec![];
    response.read_to_end(&mut sha256_data)?;

    let sha256_manifest = String::from_utf8(sha256_data)?;
    let manifest_digest_wanted = sha256_manifest
        .split(' ')
        .next()
        .ok_or_else(|| anyhow!("failed parsing SHA-256 manifest"))?
        .to_string();

    warn!("fetching {}", manifest_url);
    let mut response = client.get(&manifest_url).send()?;
    let mut manifest_data = vec![];
    response.read_to_end(&mut manifest_data)?;

    warn!("fetching {}", signature_url);
    let mut response = client.get(&signature_url).send()?;
    let mut signature_data = vec![];
    response.read_to_end(&mut signature_data)?;

    let mut hasher = sha2::Sha256::new();
    hasher.update(&manifest_data);

    let manifest_digest_got = hex::encode(hasher.finalize());

    if manifest_digest_got != manifest_digest_wanted {
        return Err(anyhow!(
            "digest mismatch on {}; wanted {}, got {}",
            manifest_url,
            manifest_digest_wanted,
            manifest_digest_got
        ));
    }

    warn!("verified SHA-256 digest for {}", manifest_url);

    let (signatures, _) = StandaloneSignature::from_armor_many(Cursor::new(&signature_data))
        .with_context(|| format!("parsing {} armored signature data", signature_url))?;

    for signature in signatures {
        let signature = signature.context("obtaining pgp signature")?;

        signature
            .verify(&*GPG_SIGNING_KEY, &manifest_data)
            .context("verifying pgp signature of manifest")?;
        warn!("verified PGP signature for {}", manifest_url);
    }

    let manifest = Manifest::from_toml_bytes(&manifest_data).context("parsing manifest TOML")?;

    Ok(manifest)
}

/// Resolve a [PackageArchive] for a requested Rust toolchain package.
///
/// This is safe to call concurrently from different threads or processes.
pub fn resolve_package_archive(
    manifest: &Manifest,
    package: &str,
    target_triple: &str,
    download_cache_dir: Option<&Path>,
) -> Result<PackageArchive> {
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
        "found Rust package {} version {} for {}",
        package, version, target_triple
    );

    let (compression_format, remote_content) = target.download_info().ok_or_else(|| {
        anyhow!(
            "package {} for target {} is not available",
            package,
            target_triple
        )
    })?;

    let tar_data = if let Some(download_dir) = download_cache_dir {
        let dest_path = download_dir.join(
            remote_content
                .url
                .rsplit('/')
                .next()
                .expect("failed to parse URL"),
        );

        download_to_path(&remote_content, &dest_path)
            .context("downloading file to cache directory")?;

        std::fs::read(&dest_path).context("reading downloaded file")?
    } else {
        download_and_verify(&remote_content)?
    };

    PackageArchive::new(compression_format, tar_data).context("obtaining PackageArchive")
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

fn materialize_archive(
    archive: &PackageArchive,
    package: &str,
    triple: &str,
    install_dir: &Path,
) -> Result<()> {
    archive.install(install_dir).context("installing")?;

    let manifest_path = install_dir.join(format!("MANIFEST.{}.{}", triple, package));
    let mut fh = std::fs::File::create(&manifest_path).context("opening manifest file")?;
    archive
        .write_installs_manifest(&mut fh)
        .context("writing installs manifest")?;

    Ok(())
}

fn sha256_path(path: &Path) -> Result<Vec<u8>> {
    let mut hasher = sha2::Sha256::new();
    let fh = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(fh);

    let mut buffer = [0; 32768];

    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(hasher.finalize().to_vec())
}

fn package_is_fresh(install_dir: &Path, package: &str, triple: &str) -> Result<bool> {
    let manifest_path = install_dir.join(format!("MANIFEST.{}.{}", triple, package));

    if !manifest_path.exists() {
        return Ok(false);
    }

    let mut fh =
        std::fs::File::open(&manifest_path).context("opening installs manifest for reading")?;
    let manifest = read_installs_manifest(&mut fh)?;

    for (path, wanted_digest) in manifest {
        let install_path = install_dir.join(&path);

        match sha256_path(&install_path) {
            Ok(got_digest) => {
                if wanted_digest != hex::encode(got_digest) {
                    return Ok(false);
                }
            }
            Err(_) => {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

/// Install a functional Rust toolchain capable of running on and building for a target triple.
///
/// This is a convenience method for fetching the packages that compose a minimal
/// Rust installation capable of compiling.
///
/// `host_triple` denotes the host triple of the toolchain to fetch.
/// `extra_target_triples` denotes extra triples for targets we are building for.
pub fn install_rust_toolchain(
    toolchain: &str,
    host_triple: &str,
    extra_target_triples: &[&str],
    install_root_dir: &Path,
    download_cache_dir: Option<&Path>,
) -> Result<InstalledToolchain> {
    let mut manifest = None;

    // The actual install directory is composed of the toolchain name and the
    // host triple.
    let install_dir = install_root_dir.join(format!("{}-{}", toolchain, host_triple));

    std::fs::create_dir_all(&install_dir)
        .with_context(|| format!("creating directory {}", install_dir.display()))?;

    let mut installs = vec![
        (host_triple, "rustc"),
        (host_triple, "cargo"),
        (host_triple, "rust-std"),
    ];

    for triple in extra_target_triples {
        if *triple != host_triple {
            installs.push((*triple, "rust-std"));
        }
    }

    let lock_path = install_dir.with_extension("lock");
    let lock = std::fs::File::create(&lock_path)
        .with_context(|| format!("creating {}", lock_path.display()))?;
    lock.lock_exclusive().context("obtaining lock")?;

    for (triple, package) in installs {
        if package_is_fresh(&install_dir, package, triple)? {
            warn!(
                "{} for {} in {} is up-to-date",
                package,
                triple,
                install_dir.display()
            );
        } else {
            if manifest.is_none() {
                manifest.replace(fetch_channel_manifest(toolchain).context("fetching manifest")?);
            }

            warn!(
                "extracting {} for {} to {}",
                package,
                triple,
                install_dir.display()
            );
            let archive = resolve_package_archive(
                manifest.as_ref().unwrap(),
                package,
                triple,
                download_cache_dir,
            )?;
            materialize_archive(&archive, package, triple, &install_dir)?;
        }
    }

    lock.unlock().context("unlocking")?;

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
    use super::*;

    static CACHE_DIR: Lazy<PathBuf> = Lazy::new(|| {
        dirs::cache_dir()
            .expect("unable to obtain cache dir")
            .join("pyoxidizer")
            .join("rust")
    });

    fn do_triple_test(target_triple: &str) -> Result<()> {
        let temp_dir = tempfile::Builder::new()
            .prefix("tugger-rust-toolchain-test")
            .tempdir()?;

        let toolchain = install_rust_toolchain(
            "stable",
            target_triple,
            &[],
            temp_dir.path(),
            Some(&*CACHE_DIR),
        )?;

        assert_eq!(
            toolchain.path,
            temp_dir.path().join(format!("stable-{}", target_triple))
        );

        // Doing it again should no-op.
        install_rust_toolchain(
            "stable",
            target_triple,
            &[],
            temp_dir.path(),
            Some(&*CACHE_DIR),
        )?;

        Ok(())
    }

    #[test]
    fn fetch_stable() -> Result<()> {
        fetch_channel_manifest("stable")?;

        Ok(())
    }

    #[test]
    fn fetch_apple() -> Result<()> {
        do_triple_test("x86_64-apple-darwin")
    }

    #[test]
    fn fetch_linux() -> Result<()> {
        do_triple_test("x86_64-unknown-linux-gnu")
    }

    #[test]
    fn fetch_windows() -> Result<()> {
        do_triple_test("x86_64-pc-windows-msvc")
    }
}
