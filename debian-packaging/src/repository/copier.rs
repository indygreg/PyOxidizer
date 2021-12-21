// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian repository copying. */

use {
    crate::{
        error::{DebianError, Result},
        io::ContentDigest,
        repository::{
            reader_from_str, writer_from_str, CopyPhase, PublishEvent, ReleaseReader,
            RepositoryRootReader, RepositoryWriteOperation, RepositoryWriter,
        },
    },
    futures::StreamExt,
    serde::{Deserialize, Serialize},
};

/// Well-known files at the root of distribution/release directories.
const RELEASE_FILES: &[&str; 4] = &["ChangeLog", "InRelease", "Release", "Release.gpg"];

/// A configuration for initializing a [RepositoryCopier].
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryCopierConfig {
    /// The URL or path of the source repository to copy from.
    pub source_url: String,

    /// The URL or path of the destination repository to copy from.
    pub destination_url: String,

    /// Names of distributions to copy.
    #[serde(default)]
    pub distributions: Vec<String>,

    /// Repository root relative paths of distributions to copy.
    #[serde(default)]
    pub distribution_paths: Vec<String>,

    /// Filter of components to copy.
    ///
    /// If not defined, all components will be copied.
    pub only_components: Option<Vec<String>>,

    /// Whether to copy binary packages.
    pub binary_packages_copy: Option<bool>,

    /// Filter of architectures of binary packages to copy.
    ///
    /// If not defined, all architectures will be copied if binary packages are
    /// being copied.
    pub binary_packages_only_architectures: Option<Vec<String>>,

    /// Whether to copy installer binary packages.
    pub installer_binary_packages_copy: Option<bool>,

    /// Filter of architectures of installer binary packages to copy.
    ///
    /// If not defined, all architectures will be copied if installer binary packages
    /// are being copied.
    pub installer_binary_packages_only_architectures: Option<Vec<String>>,

    /// Whether to copy source packages.
    pub sources_copy: Option<bool>,
}

struct GenericCopy {
    source_path: String,
    dest_path: String,
    expected_content: Option<(u64, ContentDigest)>,
}

/// Entity for copying Debian repository content.
///
/// Instances of this type can be used to copy select Debian repository content
/// between a reader and a writer.
///
/// The file layout and content is preserved, so existing PGP signatures can be preserved.
/// However, the copier does have the ability to selectively filter which files are copied.
/// So the destination repository may reference content that doesn't exist in that location.
///
/// Because repositories do not have a standardized mechanism for discovering dists/releases
/// within, this type must be told which distributions to copy. Copying is performed 1
/// distribution at a time.
///
/// By default, instances copy all copyable content. Installer files are currently not
/// supported. Incomplete copies for all other files is considered a bug and should be
/// reported.
///
/// Various `set_*` methods exist to control the copying behavior.
pub struct RepositoryCopier {
    /// Filter of components that should be copied.
    only_components: Option<Vec<String>>,

    /// Whether to copy non-installer binary packages.
    binary_packages_copy: bool,
    /// Filter of architectures of binary packages to copy.
    binary_packages_only_arches: Option<Vec<String>>,

    /// Whether to copy installer binary packages.
    installer_binary_packages_copy: bool,
    /// Filter of architectures of installer binary packages to copy.
    installer_binary_packages_only_arches: Option<Vec<String>>,

    /// Whether to copy source packages.
    sources_copy: bool,

    /// Whether to copy installers files.
    installers_copy: bool,
    /// Filter of architectures of installers to copy.
    #[allow(unused)]
    installers_only_arches: Option<Vec<String>>,
}

impl Default for RepositoryCopier {
    fn default() -> Self {
        Self {
            only_components: None,
            binary_packages_copy: true,
            binary_packages_only_arches: None,
            installer_binary_packages_copy: true,
            installer_binary_packages_only_arches: None,
            sources_copy: true,
            // TODO enable once implemented
            installers_copy: false,
            installers_only_arches: None,
        }
    }
}

impl RepositoryCopier {
    /// Set an explicit list of components whose files to copy.
    pub fn set_only_components(&mut self, components: impl Iterator<Item = String>) {
        self.only_components = Some(components.collect());
    }

    /// Set whether to copy non-installer binary packages.
    pub fn set_binary_packages_copy(&mut self, value: bool) {
        self.binary_packages_copy = value;
    }

    /// Set a filter for architectures of non-installer binary packages to copy.
    ///
    /// Binary packages for architectures not in this set will be ignored.
    pub fn set_binary_packages_only_arches(&mut self, value: impl Iterator<Item = String>) {
        self.binary_packages_only_arches = Some(value.collect::<Vec<_>>());
    }

    /// Set whether to copy installer binary packages.
    pub fn set_installer_binary_packages_copy(&mut self, value: bool) {
        self.installer_binary_packages_copy = value;
    }

    /// Set a filter for architectures of installer binary packages to copy.
    ///
    /// Binary packages for architectures not in this set will be ignored.
    pub fn set_installer_binary_packages_only_arches(
        &mut self,
        value: impl Iterator<Item = String>,
    ) {
        self.installer_binary_packages_only_arches = Some(value.collect::<Vec<_>>());
    }

    /// Set whether to copy sources package files.
    pub fn set_sources_copy(&mut self, value: bool) {
        self.sources_copy = value;
    }

    /// Perform a copy operation as defined by a [RepositoryCopierConfig].
    pub async fn copy_from_config(
        config: RepositoryCopierConfig,
        max_copy_operations: usize,
        progress_cb: &Option<Box<dyn Fn(PublishEvent) + Sync>>,
    ) -> Result<()> {
        let root_reader = reader_from_str(config.source_url)?;
        let writer = writer_from_str(config.destination_url).await?;

        let mut copier = Self::default();

        if let Some(v) = config.only_components {
            copier.set_only_components(v.into_iter());
        }
        if let Some(v) = config.binary_packages_copy {
            copier.set_binary_packages_copy(v);
        }
        if let Some(v) = config.binary_packages_only_architectures {
            copier.set_binary_packages_only_arches(v.into_iter());
        }
        if let Some(v) = config.installer_binary_packages_copy {
            copier.set_installer_binary_packages_copy(v);
        }
        if let Some(v) = config.installer_binary_packages_only_architectures {
            copier.set_installer_binary_packages_only_arches(v.into_iter());
        }
        if let Some(v) = config.sources_copy {
            copier.set_sources_copy(v);
        }

        for dist in config.distributions {
            copier
                .copy_distribution(
                    root_reader.as_ref(),
                    writer.as_ref(),
                    &dist,
                    max_copy_operations,
                    progress_cb,
                )
                .await?;
        }
        for path in config.distribution_paths {
            copier
                .copy_distribution_path(
                    root_reader.as_ref(),
                    writer.as_ref(),
                    &path,
                    max_copy_operations,
                    progress_cb,
                )
                .await?;
        }

        Ok(())
    }

    /// Copy content for a given distribution given a distribution name.
    ///
    /// This is a proxy for [Self::copy_distribution_path()] which simply passes
    /// `dists/{distribution}` as the path value. This is the standard layout for Debian
    /// repositories.
    pub async fn copy_distribution(
        &self,
        root_reader: &dyn RepositoryRootReader,
        writer: &dyn RepositoryWriter,
        distribution: &str,
        max_copy_operations: usize,
        progress_cb: &Option<Box<dyn Fn(PublishEvent) + Sync>>,
    ) -> Result<()> {
        self.copy_distribution_path(
            root_reader,
            writer,
            &format!("dists/{}", distribution),
            max_copy_operations,
            progress_cb,
        )
        .await
    }

    /// Copy content for a given distribution at a path relative to the repository root.
    ///
    /// The given `distribution_path` is usually prefixed with `dists/`. e.g. `dists/bullseye`.
    /// But it can be something else for non-standard repository layouts.
    pub async fn copy_distribution_path(
        &self,
        root_reader: &dyn RepositoryRootReader,
        writer: &dyn RepositoryWriter,
        distribution_path: &str,
        max_copy_operations: usize,
        progress_cb: &Option<Box<dyn Fn(PublishEvent) + Sync>>,
    ) -> Result<()> {
        let release = root_reader
            .release_reader_with_distribution_path(distribution_path)
            .await?;

        // We copy all the pool artifacts first because otherwise a client could fetch an indices
        // file referring to a pool file that isn't available yet.

        if self.binary_packages_copy {
            if let Some(cb) = progress_cb {
                cb(PublishEvent::CopyPhaseBegin(CopyPhase::BinaryPackages));
            }
            self.copy_binary_packages(
                root_reader,
                writer,
                release.as_ref(),
                false,
                max_copy_operations,
                progress_cb,
            )
            .await?;
            if let Some(cb) = progress_cb {
                cb(PublishEvent::CopyPhaseEnd(CopyPhase::BinaryPackages));
            }
        }

        if self.installer_binary_packages_copy {
            if let Some(cb) = progress_cb {
                cb(PublishEvent::CopyPhaseBegin(
                    CopyPhase::InstallerBinaryPackages,
                ));
            }
            self.copy_binary_packages(
                root_reader,
                writer,
                release.as_ref(),
                true,
                max_copy_operations,
                progress_cb,
            )
            .await?;
            if let Some(cb) = progress_cb {
                cb(PublishEvent::CopyPhaseEnd(
                    CopyPhase::InstallerBinaryPackages,
                ));
            }
        }

        if self.sources_copy {
            if let Some(cb) = progress_cb {
                cb(PublishEvent::CopyPhaseBegin(CopyPhase::Sources));
            }
            self.copy_source_packages(
                root_reader,
                writer,
                release.as_ref(),
                max_copy_operations,
                progress_cb,
            )
            .await?;
            if let Some(cb) = progress_cb {
                cb(PublishEvent::CopyPhaseEnd(CopyPhase::Sources));
            }
        }

        if self.installers_copy {
            if let Some(cb) = progress_cb {
                cb(PublishEvent::CopyPhaseBegin(CopyPhase::Installers));
            }
            self.copy_installers(
                root_reader,
                writer,
                release.as_ref(),
                max_copy_operations,
                progress_cb,
            )
            .await?;
            if let Some(cb) = progress_cb {
                cb(PublishEvent::CopyPhaseEnd(CopyPhase::Installers));
            }
        }

        // All the pool artifacts are in place. Publish the indices files.

        if let Some(cb) = progress_cb {
            cb(PublishEvent::CopyPhaseBegin(CopyPhase::ReleaseIndices));
        }
        self.copy_release_indices(
            root_reader,
            writer,
            release.as_ref(),
            max_copy_operations,
            progress_cb,
        )
        .await?;
        if let Some(cb) = progress_cb {
            cb(PublishEvent::CopyPhaseEnd(CopyPhase::ReleaseIndices));
        }

        // And finally publish the Release files.
        if let Some(cb) = progress_cb {
            cb(PublishEvent::CopyPhaseBegin(CopyPhase::ReleaseFiles));
        }
        self.copy_release_files(
            root_reader,
            writer,
            distribution_path,
            max_copy_operations,
            progress_cb,
        )
        .await?;
        if let Some(cb) = progress_cb {
            cb(PublishEvent::CopyPhaseEnd(CopyPhase::ReleaseFiles));
        }

        Ok(())
    }

    async fn copy_binary_packages(
        &self,
        root_reader: &dyn RepositoryRootReader,
        writer: &dyn RepositoryWriter,
        release: &dyn ReleaseReader,
        installer_packages: bool,
        max_copy_operations: usize,
        progress_cb: &Option<Box<dyn Fn(PublishEvent) + Sync>>,
    ) -> Result<()> {
        let only_arches = if installer_packages {
            self.installer_binary_packages_only_arches.clone()
        } else {
            self.binary_packages_only_arches.clone()
        };
        let only_components = self.only_components.clone();

        let copies = release
            .resolve_package_fetches(
                Box::new(move |entry| {
                    let component_allowed = if let Some(only_components) = &only_components {
                        only_components.contains(&entry.component.to_string())
                    } else {
                        true
                    };

                    let arch_allowed = if let Some(only_arches) = &only_arches {
                        only_arches.contains(&entry.architecture.to_string())
                    } else {
                        true
                    };

                    component_allowed && arch_allowed && entry.is_installer == installer_packages
                }),
                Box::new(move |_| true),
                max_copy_operations,
            )
            .await?
            .into_iter()
            .map(|bpf| GenericCopy {
                source_path: bpf.path.clone(),
                dest_path: bpf.path,
                expected_content: Some((bpf.size, bpf.digest)),
            })
            .collect::<Vec<_>>();

        perform_copies(
            root_reader,
            writer,
            copies,
            max_copy_operations,
            false,
            progress_cb,
        )
        .await?;

        Ok(())
    }

    async fn copy_source_packages(
        &self,
        root_reader: &dyn RepositoryRootReader,
        writer: &dyn RepositoryWriter,
        release: &dyn ReleaseReader,
        max_copy_operations: usize,
        progress_cb: &Option<Box<dyn Fn(PublishEvent) + Sync>>,
    ) -> Result<()> {
        let only_components = self.only_components.clone();

        let copies = release
            .resolve_source_fetches(
                Box::new(move |entry| {
                    if let Some(only_components) = &only_components {
                        only_components.contains(&entry.component.to_string())
                    } else {
                        true
                    }
                }),
                Box::new(move |_| true),
                max_copy_operations,
            )
            .await?
            .into_iter()
            .map(|spf| GenericCopy {
                source_path: spf.path.clone(),
                dest_path: spf.path.clone(),
                expected_content: Some((spf.size, spf.digest.clone())),
            })
            .collect::<Vec<_>>();

        perform_copies(
            root_reader,
            writer,
            copies,
            max_copy_operations,
            false,
            progress_cb,
        )
        .await?;

        Ok(())
    }

    async fn copy_installers(
        &self,
        _root_reader: &dyn RepositoryRootReader,
        _writer: &dyn RepositoryWriter,
        _release: &dyn ReleaseReader,
        _max_copy_operations: usize,
        _progress_cb: &Option<Box<dyn Fn(PublishEvent) + Sync>>,
    ) -> Result<()> {
        // Not yet supported since this requires teaching content validating fetching about
        // optional sizes.
        todo!();
    }

    async fn copy_release_indices(
        &self,
        root_reader: &dyn RepositoryRootReader,
        writer: &dyn RepositoryWriter,
        release: &dyn ReleaseReader,
        max_copy_operations: usize,
        progress_cb: &Option<Box<dyn Fn(PublishEvent) + Sync>>,
    ) -> Result<()> {
        let by_hash = release.release_file().acquire_by_hash().unwrap_or(false);

        let copies = release
            .classified_indices_entries()?
            .into_iter()
            .filter(|_| {
                // For now we always copy all the indices files. There is certainly room to filter the
                // files that are copied. We can do that when we feel like writing the code...
                true
            })
            .map(move |entry| {
                let path = if by_hash {
                    entry.by_hash_path()
                } else {
                    entry.path.to_string()
                };

                let path = format!("{}/{}", release.root_relative_path(), path);

                GenericCopy {
                    source_path: path.clone(),
                    dest_path: path,
                    expected_content: Some((entry.size, entry.digest.clone())),
                }
            })
            .collect::<Vec<_>>();

        // Some indices files don't actually exist! For example, the release file will publish
        // the checksums of the uncompressed file variant but the uncompressed file won't
        // actually be present in the repository! These errors are OK to ignore. But we still
        // report on them.
        perform_copies(
            root_reader,
            writer,
            copies,
            max_copy_operations,
            true,
            progress_cb,
        )
        .await?;

        Ok(())
    }

    async fn copy_release_files(
        &self,
        root_reader: &dyn RepositoryRootReader,
        writer: &dyn RepositoryWriter,
        distribution_path: &str,
        max_copy_operations: usize,
        progress_cb: &Option<Box<dyn Fn(PublishEvent) + Sync>>,
    ) -> Result<()> {
        let copies = RELEASE_FILES
            .iter()
            .map(|path| {
                let path = format!("{}/{}", distribution_path, path);

                GenericCopy {
                    source_path: path.clone(),
                    dest_path: path,
                    expected_content: None,
                }
            })
            .collect::<Vec<_>>();

        // Not all the well-known files exist. So ignore missing file errors.
        // TODO we probably want a hard error if `Release` or `InRelease` fail.
        perform_copies(
            root_reader,
            writer,
            copies,
            max_copy_operations,
            true,
            progress_cb,
        )
        .await?;

        Ok(())
    }
}

/// Perform a sequence of copy operations between a reader and writer.
async fn perform_copies(
    root_reader: &dyn RepositoryRootReader,
    writer: &dyn RepositoryWriter,
    copies: Vec<GenericCopy>,
    max_copy_operations: usize,
    allow_not_found: bool,
    progress_cb: &Option<Box<dyn Fn(PublishEvent) + Sync>>,
) -> Result<()> {
    let mut total_size = 0;

    let fs = copies
        .into_iter()
        .map(|op| {
            if let Some((size, _)) = op.expected_content {
                total_size += size;
            }

            writer.copy_from(
                root_reader,
                op.source_path.into(),
                op.expected_content,
                op.dest_path.into(),
                progress_cb,
            )
        })
        .collect::<Vec<_>>();

    if let Some(cb) = progress_cb {
        cb(PublishEvent::WriteSequenceBeginWithTotalBytes(total_size));
    }

    let mut buffered = futures::stream::iter(fs).buffer_unordered(max_copy_operations);

    while let Some(res) = buffered.next().await {
        match res {
            Ok(write) => {
                if let Some(cb) = progress_cb {
                    cb(PublishEvent::WriteSequenceProgressBytes(
                        write.bytes_written(),
                    ));

                    match write {
                        RepositoryWriteOperation::PathWritten(write) => {
                            cb(PublishEvent::PathCopied(
                                write.path.to_string(),
                                write.bytes_written,
                            ));
                        }
                        RepositoryWriteOperation::Noop(path, _) => {
                            cb(PublishEvent::PathCopyNoop(path.to_string()));
                        }
                    }
                }
            }
            Err(DebianError::RepositoryIoPath(path, err))
                if allow_not_found && matches!(err.kind(), std::io::ErrorKind::NotFound) =>
            {
                if let Some(cb) = progress_cb {
                    cb(PublishEvent::CopyIndicesPathNotFound(path));
                }
            }
            Err(e) => return Err(e),
        }
    }

    if let Some(cb) = progress_cb {
        cb(PublishEvent::WriteSequenceFinished);
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use {
        super::*,
        crate::repository::{
            http::HttpRepositoryClient,
            proxy_writer::{ProxyVerifyBehavior, ProxyWriter},
            sink_writer::SinkWriter,
        },
    };

    const DEBIAN_URL: &str = "http://snapshot.debian.org/archive/debian/20211120T085721Z";

    #[tokio::test]
    async fn bullseye_copy() -> Result<()> {
        let root =
            Box::new(HttpRepositoryClient::new(DEBIAN_URL)?) as Box<dyn RepositoryRootReader>;
        let mut writer = ProxyWriter::new(SinkWriter::default());
        writer.set_verify_behavior(ProxyVerifyBehavior::AlwaysExistsIntegrityVerified);
        let writer: Box<dyn RepositoryWriter> = Box::new(writer);

        let mut copier = RepositoryCopier::default();
        copier.set_binary_packages_copy(false);
        copier.set_installer_binary_packages_copy(false);
        copier.set_sources_copy(false);

        let cb = Box::new(|_| {});

        copier
            .copy_distribution(root.as_ref(), writer.as_ref(), "bullseye", 8, &Some(cb))
            .await?;

        Ok(())
    }
}
