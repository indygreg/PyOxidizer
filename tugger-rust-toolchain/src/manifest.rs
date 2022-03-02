// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! TOML manifests.

use {
    crate::tar::CompressionFormat,
    anyhow::{anyhow, Result},
    std::collections::HashMap,
    tugger_common::http::RemoteContent,
};

/// Represents a toolchain manifest file.
#[derive(Clone, Debug)]
pub struct Manifest {
    pub packages: HashMap<String, Package>,
}

impl Manifest {
    /// Obtain an instance by parsing TOML bytes.
    pub fn from_toml_bytes(data: &[u8]) -> Result<Self> {
        let table = toml::from_slice(data)?;

        Self::from_toml(table)
    }

    pub fn from_toml(table: toml::value::Table) -> Result<Self> {
        let manifest_version = match table
            .get("manifest-version")
            .ok_or_else(|| anyhow!("manifest TOML doesn't have manifest-version key"))?
        {
            toml::Value::String(s) => s,
            _ => return Err(anyhow!("failed to obtain manifest-version from TOML")),
        };

        if manifest_version != "2" {
            return Err(anyhow!(
                "unhandled manifest-version: {}; only version 2 supported",
                manifest_version,
            ));
        }

        let packages = Self::parse_packages(table)?;

        Ok(Self { packages })
    }

    pub fn parse_packages(mut table: toml::value::Table) -> Result<HashMap<String, Package>> {
        let mut result = HashMap::new();

        let pkg_table = match table
            .remove("pkg")
            .ok_or_else(|| anyhow!("manifest TOML doesn't have any [pkg]"))?
        {
            toml::Value::Table(table) => table,
            _ => return Err(anyhow!("manifest TOML doesn't have table [pkg]")),
        };

        for (k, v) in pkg_table {
            if let toml::Value::Table(v) = v {
                result.insert(k.clone(), Package::from_table(v)?);
            }
        }

        Ok(result)
    }

    /// Find a package for a target triple in this manifest.
    pub fn find_package(
        &self,
        package: &str,
        target_triple: &str,
    ) -> Option<(String, ManifestTargetedPackage)> {
        match self.packages.get(package) {
            Some(package) => match &package.target {
                PackageTarget::Wildcard => None,
                PackageTarget::Targeted(targets) => targets
                    .get(target_triple)
                    .map(|target| (package.version.clone(), target.clone())),
            },
            None => None,
        }
    }
}

/// Represents a `[pkg]` entry in a toolchain manifest TOML.
#[derive(Clone, Debug)]
pub struct Package {
    pub version: String,
    pub target: PackageTarget,
}

impl Package {
    pub fn from_table(mut table: toml::value::Table) -> Result<Self> {
        let version = match table
            .remove("version")
            .ok_or_else(|| anyhow!("[pkg] doesn't have version"))?
        {
            toml::Value::String(v) => v,
            _ => return Err(anyhow!("pkg TOML has non-string version")),
        };

        let mut target_table = match table
            .remove("target")
            .ok_or_else(|| anyhow!("[pkg] does not have .target table"))?
        {
            toml::Value::Table(t) => t,
            _ => return Err(anyhow!("[pkg.target] is not a table")),
        };

        let target = if let Some(toml::Value::Table(_)) = target_table.remove("*") {
            PackageTarget::Wildcard
        } else {
            let mut targets = HashMap::new();

            for (k, v) in target_table {
                if let toml::Value::Table(mut v) = v {
                    let available = v
                        .remove("available")
                        .ok_or_else(|| anyhow!("available not set"))?
                        .as_bool()
                        .ok_or_else(|| anyhow!("available not a bool"))?;

                    let mut urls = vec![];

                    for prefix in &["zst_", "xz_", ""] {
                        let url = v.remove(&format!("{}url", prefix));
                        let hash = v.remove(&format!("{}hash", prefix));

                        if let (Some(url), Some(hash)) = (url, hash) {
                            let url = url
                                .as_str()
                                .ok_or_else(|| anyhow!("url not a string"))?
                                .to_string();
                            let hash = hash
                                .as_str()
                                .ok_or_else(|| anyhow!("hash not a string"))?
                                .to_string();

                            urls.push((
                                match *prefix {
                                    "zst_" => CompressionFormat::Zstd,
                                    "xz_" => CompressionFormat::Xz,
                                    "" => CompressionFormat::Gzip,
                                    _ => panic!("logic error in compression format handling"),
                                },
                                url,
                                hash,
                            ));
                        }
                    }

                    targets.insert(
                        k.clone(),
                        ManifestTargetedPackage {
                            name: k,
                            available,
                            urls,
                        },
                    );
                }
            }

            PackageTarget::Targeted(targets)
        };

        Ok(Self { version, target })
    }
}

#[derive(Clone, Debug)]
pub enum PackageTarget {
    Wildcard,
    Targeted(HashMap<String, ManifestTargetedPackage>),
}

#[derive(Clone, Debug)]
pub struct ManifestTargetedPackage {
    pub name: String,
    pub available: bool,
    pub urls: Vec<(CompressionFormat, String, String)>,
}

impl ManifestTargetedPackage {
    pub fn download_info(&self) -> Option<(CompressionFormat, RemoteContent)> {
        let (format, url, digest) = self.urls.get(0)?;

        Some((
            *format,
            RemoteContent {
                name: self.name.clone(),
                url: url.to_string(),
                sha256: digest.to_string(),
            },
        ))
    }
}
