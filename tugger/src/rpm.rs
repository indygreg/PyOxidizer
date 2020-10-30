// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::file_resource::FileManifest,
    anyhow::{anyhow, Context, Result},
    rpm::{RPMFileOptions, RPMPackage},
    std::path::{Path, PathBuf},
};

/// Create RPMs.
///
/// This is a thin wrapper around rpm::RPMBuilder which provides some
/// minor quality of life improvements, such as handling of
/// `FileManifest` instances.
pub struct RPMBuilder {
    inner: rpm::RPMBuilder,

    build_path: PathBuf,
    files: FileManifest,
}

impl AsMut<rpm::RPMBuilder> for RPMBuilder {
    fn as_mut(&mut self) -> &mut rpm::RPMBuilder {
        &mut self.inner
    }
}

impl RPMBuilder {
    /// Create a new instance from required fields.
    pub fn new<P: AsRef<Path>>(
        build_path: P,
        name: &str,
        version: &str,
        license: &str,
        arch: &str,
        description: &str,
    ) -> Self {
        let inner = rpm::RPMBuilder::new(name, version, license, arch, description);

        Self {
            inner,
            build_path: build_path.as_ref().to_path_buf(),
            files: FileManifest::default(),
        }
    }

    /// Populate registered files with the internal RPMBuilder.
    pub fn populate_files(mut self) -> Result<Self> {
        self.files
            .write_to_path(&self.build_path)
            .context("writing RPM data files")?;

        for (rel_path, content) in self.files.entries() {
            let real_path = self.build_path.join(rel_path);

            let mut options = RPMFileOptions::new(rel_path.display().to_string());

            if content.executable {
                options = options.mode(0o100_775);
            }

            // TODO support additional attributes, such as owner/group.
            // TODO make deterministic by modifying upstream to allow control
            // over what add_data() does.

            self.inner = self
                .inner
                .with_file(&real_path, options)
                .map_err(|e| anyhow!("error registering file with RPMBuilder: {}", e))
                .context("registering file with RPM")?;
        }

        Ok(self)
    }

    /// Build the RPM, consuming self.
    pub fn build(mut self) -> Result<RPMPackage> {
        self = self
            .populate_files()
            .context("populating files with builder")?;

        let package = self
            .inner
            .build()
            .map_err(|e| anyhow!("error building RPM: {}", e))
            .context("building RPM")?;

        Ok(package)
    }

    /// Build the RPM, writing it to a filesystem path, consuming self.
    pub fn build_to_path<P: AsRef<Path>>(self, dest_path: P) -> Result<()> {
        let package = self.build()?;

        let dest_path = dest_path.as_ref();

        if let Some(parent) = dest_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let mut f = std::fs::File::create(dest_path)?;
        package
            .write(&mut f)
            .map_err(|e| anyhow!("error writing RPM: {}", e))?;

        Ok(())
    }
}
