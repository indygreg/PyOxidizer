// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian repository copying. */

use {
    crate::{
        error::{DebianError, Result},
        io::ContentDigest,
        repository::{PublishEvent, ReleaseReader, RepositoryRootReader, RepositoryWriter},
    },
    futures::StreamExt,
};

/// Well-known files at the root of distribution/release directories.
const RELEASE_FILES: &[&str; 4] = &["ChangeLog", "InRelease", "Release", "Release.gpg"];

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

    /// Copy content for a given distribution given a distribution name.
    ///
    /// This is a proxy for [Self::copy_distribution_path()] which simply passes
    /// `dists/{distribution}` as the path value. This is the standard layout for Debian
    /// repositories.
    pub async fn copy_distribution<F>(
        &self,
        root_reader: &Box<dyn RepositoryRootReader>,
        writer: &impl RepositoryWriter,
        distribution: &str,
        threads: usize,
        progress_cb: &Option<F>,
    ) -> Result<()>
    where
        F: Fn(PublishEvent) + Sync,
    {
        self.copy_distribution_path(
            root_reader,
            writer,
            &format!("dists/{}", distribution),
            threads,
            progress_cb,
        )
        .await
    }

    /// Copy content for a given distribution at a path relative to the repository root.
    ///
    /// The given `distribution_path` is usually prefixed with `dists/`. e.g. `dists/bullseye`.
    /// But it can be something else for non-standard repository layouts.
    pub async fn copy_distribution_path<F>(
        &self,
        root_reader: &Box<dyn RepositoryRootReader>,
        writer: &impl RepositoryWriter,
        distribution_path: &str,
        threads: usize,
        progress_cb: &Option<F>,
    ) -> Result<()>
    where
        F: Fn(PublishEvent) + Sync,
    {
        let release = root_reader
            .release_reader_with_distribution_path(distribution_path)
            .await?;

        // We copy all the pool artifacts first because otherwise a client could fetch an indices
        // file referring to a pool file that isn't available yet.

        if self.binary_packages_copy {
            self.copy_binary_packages(root_reader, writer, &release, false, threads, progress_cb)
                .await?;
        }

        if self.installer_binary_packages_copy {
            self.copy_binary_packages(root_reader, writer, &release, true, threads, progress_cb)
                .await?;
        }

        if self.sources_copy {
            self.copy_source_packages(root_reader, writer, &release, threads, progress_cb)
                .await?;
        }

        if self.installers_copy {
            self.copy_installers(root_reader, writer, &release, threads, progress_cb)
                .await?;
        }

        // All the pool artifacts are in place. Publish the indices files.

        self.copy_release_indices(root_reader, writer, &release, threads, progress_cb)
            .await?;

        // And finally publish the Release files.
        self.copy_release_files(root_reader, writer, distribution_path, threads, progress_cb)
            .await?;

        Ok(())
    }

    async fn copy_binary_packages<F>(
        &self,
        root_reader: &Box<dyn RepositoryRootReader>,
        writer: &impl RepositoryWriter,
        release: &Box<dyn ReleaseReader>,
        installer_packages: bool,
        threads: usize,
        progress_cb: &Option<F>,
    ) -> Result<()>
    where
        F: Fn(PublishEvent) + Sync,
    {
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
                threads,
            )
            .await?
            .into_iter()
            .map(|bpf| GenericCopy {
                source_path: bpf.path.clone(),
                dest_path: bpf.path,
                expected_content: Some((bpf.size, bpf.digest)),
            })
            .collect::<Vec<_>>();

        perform_copies(root_reader, writer, copies, threads, false, progress_cb).await?;

        Ok(())
    }

    async fn copy_source_packages<F>(
        &self,
        root_reader: &Box<dyn RepositoryRootReader>,
        writer: &impl RepositoryWriter,
        release: &Box<dyn ReleaseReader>,
        threads: usize,
        progress_cb: &Option<F>,
    ) -> Result<()>
    where
        F: Fn(PublishEvent) + Sync,
    {
        // TODO There's probably a way to filter components in here.
        let copies = release
            .resolve_source_fetches(Box::new(move |_| true), Box::new(move |_| true), threads)
            .await?
            .into_iter()
            .map(|spf| GenericCopy {
                source_path: spf.path.clone(),
                dest_path: spf.path.clone(),
                expected_content: Some((spf.size, spf.digest.clone())),
            })
            .collect::<Vec<_>>();

        perform_copies(root_reader, writer, copies, threads, false, progress_cb).await?;

        Ok(())
    }

    async fn copy_installers<F>(
        &self,
        _root_reader: &Box<dyn RepositoryRootReader>,
        _writer: &impl RepositoryWriter,
        _release: &Box<dyn ReleaseReader>,
        _threads: usize,
        _progress_cb: &Option<F>,
    ) -> Result<()>
    where
        F: Fn(PublishEvent) + Sync,
    {
        // Not yet supported since this requires teaching content validating fetching about
        // optional sizes.
        todo!();
    }

    async fn copy_release_indices<F>(
        &self,
        root_reader: &Box<dyn RepositoryRootReader>,
        writer: &impl RepositoryWriter,
        release: &Box<dyn ReleaseReader>,
        threads: usize,
        progress_cb: &Option<F>,
    ) -> Result<()>
    where
        F: Fn(PublishEvent) + Sync,
    {
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
        perform_copies(root_reader, writer, copies, threads, true, progress_cb).await?;

        Ok(())
    }

    async fn copy_release_files<F>(
        &self,
        root_reader: &Box<dyn RepositoryRootReader>,
        writer: &impl RepositoryWriter,
        distribution_path: &str,
        threads: usize,
        progress_cb: &Option<F>,
    ) -> Result<()>
    where
        F: Fn(PublishEvent) + Sync,
    {
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
        perform_copies(root_reader, writer, copies, threads, true, progress_cb).await?;

        Ok(())
    }
}

/// Perform a sequence of copy operations between a reader and writer.
async fn perform_copies<F>(
    root_reader: &Box<dyn RepositoryRootReader>,
    writer: &impl RepositoryWriter,
    copies: Vec<GenericCopy>,
    threads: usize,
    allow_not_found: bool,
    progress_cb: &Option<F>,
) -> Result<()>
where
    F: Fn(PublishEvent) + Sync,
{
    let fs = copies
        .into_iter()
        .map(|op| {
            writer.copy_from(
                root_reader,
                op.source_path.into(),
                op.expected_content,
                op.dest_path.into(),
                progress_cb,
            )
        })
        .collect::<Vec<_>>();

    let mut buffered = futures::stream::iter(fs).buffer_unordered(threads);

    while let Some(res) = buffered.next().await {
        match res {
            Ok(_) => {}
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

        let mut copier = RepositoryCopier::default();
        copier.set_binary_packages_copy(false);
        copier.set_installer_binary_packages_copy(false);
        copier.set_sources_copy(false);

        let cb = |_| {};

        copier
            .copy_distribution(&root, &writer, "bullseye", 8, &Some(cb))
            .await?;

        Ok(())
    }
}
