// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Functionality for creating wheels.

use {
    anyhow::{anyhow, Context, Result},
    once_cell::sync::Lazy,
    sha2::Digest,
    simple_file_manifest::{FileEntry, FileManifest},
    std::{
        cmp::Ordering,
        io::{Seek, Write},
        path::{Path, PathBuf},
    },
};

/// Wheel filename component escape regular expression.
static RE_FILENAME_ESCAPE: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"[^\w\d.]+").unwrap());

fn base64_engine() -> impl base64::engine::Engine {
    base64::engine::fast_portable::FastPortable::from(
        &base64::alphabet::URL_SAFE,
        base64::engine::fast_portable::FastPortableConfig::new().with_encode_padding(false),
    )
}

/// Define and build a Python wheel from raw components.
///
/// Python wheels are glorified zip files with some special files
/// annotating the Python component therein.
///
/// # Wheel Level Parameters
///
/// Wheels are defined by a *distribution* (e.g. a Python package name),
/// a *version*, a *compatibility tag*, and an optional *build tag*.
///
/// The *compatibility tag* defines the Python, ABI, and platform
/// compatibility of the wheel. See
/// [PEP 425](https://www.python.org/dev/peps/pep-0425/) for an overview of the
/// components of the compatibility tag and their potential values.
///
/// Our default *compatibility tag* value is `py3-none-any`. This is
/// appropriate for a wheel containing pure Python code that is compatible
/// with Python 3. If your wheel has binary executables or extension modules,
/// you will want to update the compatibility tag to reflect the appropriate
/// binary compatibility.
///
/// # .dist-info/WHEEL File
///
/// Wheel archives must have a `WHEEL` file describing the wheel itself.
///
/// This file is an email header like MIME document with various well-defined
/// fields.
///
/// By default, we will automatically derive a minimal `WHEEL` file based
/// on parameters passed into [Self::new] and defaults.
///
/// If you want to provide your own `WHEEL` file, simply define its content
/// by adding a custom file through [Self::add_file_dist_info].
///
/// # .dist-info/METADATA File
///
/// Wheel archives must have a `METADATA` file describing the thing being
/// distributed.
///
/// This file is an email header like MIME document with various well-defined
/// fields.
///
/// By default, we will automatically derive a minimal `METADATA` file
/// based on builder state.
///
/// If you want to provide your own `METADATA` file, simply define its content
/// by adding a custom file through [Self::add_file_dist_info].
///
/// # Adding Files
///
/// Files in wheels go in 1 of 3 locations:
///
/// 1. The `.dist-info/` directory (added via [Self::add_file_dist_info]).
/// 2. Special `.data/<location>/` directories (added via [Self::add_file_data]).
/// 3. Everywhere else (added via [Self::add_file]).
///
/// Files in `.dist-info/` describe the wheel itself and the entity being
/// distributed.
///
/// Files in `.data/<location>/` are moved to the indicated `<location>` when the
/// wheel is installed. `<location>` here is the name of a Python installation
/// directory, such as `purelib` (pure Python modules and bytecode), `platlib`
/// (platform-specific / binary Python extension modules and other binaries),
/// `scripts` (executable scripts), and more.
///
/// Files in all other locations in the archive are not treated specially and are
/// extracted directly to `purelib` or `platlib`, depending on the value of
/// `Root-Is-Purelib`.
///
/// # Building Wheels
///
/// Once you have modified settings and registered files, it is time to create your
/// wheel.
///
/// If you want to materialize a `.whl` file with the proper file name, call
/// [Self::write_wheel_into_directory].
///
/// If you want to just materialize the zip content of the wheel, call
/// [Self::write_wheel_data].
///
/// If you want to obtain a collection of all the files that constitute the wheel
/// before zip file generation, call [Self::build_file_manifest].
///
/// To obtain the name of the `.whl` file given current settings, call
/// [Self::wheel_file_name].
///
/// Wheel zip archive content is deterministic for the same builder instance.
/// For separate builder instances, content can be made identical by calling
/// [Self::set_modified_time] to set the modified time and using identical input
/// settings/files. (The modified time of files in zip files defaults to the time
/// when the builder instance was created, which is obviously not deterministic.)
///
/// # Validation
///
/// This type generally performs little to no validation of input data. It is up
/// to the caller to supply settings and content that constitutes a well-formed
/// wheel.
///
/// Supplementary tools like [auditwheel](https://pypi.org/project/auditwheel/) can
/// be useful for validating the content of wheels.
pub struct WheelBuilder {
    /// The primary name of the wheel.
    distribution: String,

    /// The version component of the wheel.
    version: String,

    /// Tag denoting the build of this wheel.
    build_tag: Option<String>,

    /// Python part of compatibility tag.
    python_tag: String,

    /// ABI part of compatibility tag.
    abi_tag: String,

    /// Platform part of compatibility tag.
    platform_tag: String,

    /// Name of tool that generated this wheel.
    generator: String,

    /// Whether archive should be extracted directly into purelib.
    root_is_purelib: bool,

    /// Files constituting the wheel.
    manifest: FileManifest,

    /// The modified time to write for files in the wheel archive.
    modified_time: time::OffsetDateTime,
}

impl WheelBuilder {
    /// Create a new instance with a package name and version.
    pub fn new(distribution: impl ToString, version: impl ToString) -> Self {
        Self {
            distribution: distribution.to_string(),
            version: version.to_string(),
            build_tag: None,
            python_tag: "py3".to_string(),
            abi_tag: "none".to_string(),
            platform_tag: "any".to_string(),
            generator: "rust-python-packaging".to_string(),
            root_is_purelib: false,
            manifest: FileManifest::default(),
            modified_time: time::OffsetDateTime::now_utc(),
        }
    }

    /// Obtain the build tag for this wheel.
    pub fn build_tag(&self) -> Option<&str> {
        self.build_tag.as_deref()
    }

    /// Set the build tag for this wheel.
    pub fn set_build_tag(&mut self, v: impl ToString) {
        self.build_tag = Some(v.to_string());
    }

    /// Obtain the compatibility tag.
    pub fn tag(&self) -> String {
        format!("{}-{}-{}", self.python_tag, self.abi_tag, self.platform_tag)
    }

    /// Set the compatibility tag from a value.
    pub fn set_tag(&mut self, tag: impl ToString) -> Result<()> {
        let tag = tag.to_string();

        let mut parts = tag.splitn(3, '-');

        let python = parts
            .next()
            .ok_or_else(|| anyhow!("could not parse Python tag"))?;
        let abi = parts
            .next()
            .ok_or_else(|| anyhow!("could not parse ABI tag"))?;
        let platform = parts
            .next()
            .ok_or_else(|| anyhow!("could not parse Platform tag"))?;

        self.set_python_tag(python);
        self.set_abi_tag(abi);
        self.set_platform_tag(platform);

        Ok(())
    }

    /// Obtain the Python component of the compatibility tag.
    pub fn python_tag(&self) -> &str {
        &self.python_tag
    }

    /// Set the Python component of the compatibility tag.
    pub fn set_python_tag(&mut self, v: impl ToString) {
        self.python_tag = v.to_string();
    }

    /// Obtain the ABI component of the compatibility tag.
    pub fn abi_tag(&self) -> &str {
        &self.abi_tag
    }

    /// Set the ABI component of the compatibility tag.
    pub fn set_abi_tag(&mut self, v: impl ToString) {
        self.abi_tag = v.to_string();
    }

    /// Obtain the platform component of the compatibility tag.
    pub fn platform_tag(&self) -> &str {
        &self.platform_tag
    }

    /// Set the platform component of the compatibility tag.
    pub fn set_platform_tag(&mut self, v: impl ToString) {
        self.platform_tag = v.to_string();
    }

    /// Obtain the `Generator` value for the `WHEEL` file.
    pub fn generator(&self) -> &str {
        &self.generator
    }

    /// Set the `Generator` value for the `WHEEL` file.
    pub fn set_generator(&mut self, v: impl ToString) {
        self.generator = v.to_string();
    }

    /// Obtain the `Root-Is-Purelib` value.
    pub fn root_is_purelib(&self) -> bool {
        self.root_is_purelib
    }

    /// Set the value for `Root-Is-Purelib`.
    ///
    /// If `true`, the wheel archive is extracted directly into `purelib`. If `false`,
    /// it is extracted to `platlib`.
    pub fn set_root_is_purelib(&mut self, v: bool) {
        self.root_is_purelib = v;
    }

    /// Obtain the modified time for files in the wheel archive.
    pub fn modified_time(&self) -> time::OffsetDateTime {
        self.modified_time
    }

    /// Set the modified time for files in the wheel archive.
    pub fn set_modified_time(&mut self, v: time::OffsetDateTime) {
        self.modified_time = v;
    }

    fn normalized_distribution(&self) -> String {
        self.distribution.to_lowercase().replace('-', "_")
    }

    fn dist_info_path(&self) -> PathBuf {
        PathBuf::from(format!(
            "{}-{}.dist-info",
            self.normalized_distribution(),
            self.version
        ))
    }

    /// Add a file to the wheel at the given path.
    ///
    /// No validation of the path is performed.
    pub fn add_file(&mut self, path: impl AsRef<Path>, file: impl Into<FileEntry>) -> Result<()> {
        self.manifest.add_file_entry(path, file)?;

        Ok(())
    }

    /// Add a file to the `.dist-info/` directory.
    ///
    /// Attempts to add the `RECORD` file will work. However, the content will be
    /// ignored and regenerated as part of wheel building.
    pub fn add_file_dist_info(
        &mut self,
        path: impl AsRef<Path>,
        file: impl Into<FileEntry>,
    ) -> Result<()> {
        self.manifest
            .add_file_entry(self.dist_info_path().join(path), file)?;

        Ok(())
    }

    /// Add a file to a `.data/<destination>/` directory.
    ///
    /// `destination` is the name of a well-known Python installation directory. e.g.
    /// `{purelib, platlib, headers, scripts, data}`. When the wheel is installed,
    /// files in these `.data/<destination>/` directories are moved to the corresponding
    /// path location within the targeted environment.
    ///
    /// No validation of the `destination` values is performed.
    pub fn add_file_data(
        &mut self,
        destination: impl ToString,
        path: impl AsRef<Path>,
        file: impl Into<FileEntry>,
    ) -> Result<()> {
        self.manifest.add_file_entry(
            PathBuf::from(format!(
                "{}-{}.data",
                self.normalized_distribution(),
                self.version
            ))
            .join(destination.to_string())
            .join(path),
            file,
        )?;

        Ok(())
    }

    /// Construct the contents of the `.dist-info/WHEEL` file.
    fn derive_wheel_file(&self) -> String {
        format!(
            "Wheel-Version: 1.0\nGenerator: {}\nRoot-Is-Purelib: {}\nTag: {}\n",
            self.generator,
            self.root_is_purelib,
            self.tag()
        )
    }

    fn derive_metadata_file(&self) -> String {
        format!(
            "Metadata-Version: 2.1\nName: {}\nVersion: {}\n",
            self.distribution, self.version
        )
    }

    /// Derive the content of a `.dist-info/RECORD` file in a wheel.
    ///
    /// This iterates the contents of a [FileManifest] and derives digests and
    /// other metadata and assembles it into the appropriate format.
    pub fn derive_record_file(&self, manifest: &FileManifest) -> Result<String> {
        let mut lines = manifest
            .iter_entries()
            .map(|(path, entry)| {
                let content = entry
                    .resolve_content()
                    .with_context(|| format!("resolving content for {}", path.display()))?;

                let mut digest = sha2::Sha256::new();
                digest.update(&content);

                Ok(format!(
                    "{},sha256={},{}",
                    path.display(),
                    base64::encode_engine(digest.finalize().as_slice(), &base64_engine()),
                    content.len()
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        lines.push(format!("{}/RECORD,,\n", self.dist_info_path().display()));

        Ok(lines.join("\n"))
    }

    /// Obtain the file name for this wheel, as currently configured.
    ///
    /// The file name of a wheel is of the form
    /// `{distribution}-{version}(-{build tag})?-{python tag}-{abi tag}-{platform tag}.whl`,
    /// per PEP 427. Each component is escaped with a regular expression.
    pub fn wheel_file_name(&self) -> String {
        let mut parts = vec![self.normalized_distribution(), self.version.clone()];

        if let Some(v) = &self.build_tag {
            parts.push(v.clone());
        }

        parts.push(self.python_tag.clone());
        parts.push(self.abi_tag.clone());
        parts.push(self.platform_tag.clone());

        let s = parts
            .iter()
            .map(|x| RE_FILENAME_ESCAPE.replace_all(x, "_"))
            .collect::<Vec<_>>()
            .join("-");

        format!("{}.whl", s)
    }

    /// Obtain a [FileManifest] holding the contents of the built wheel.
    ///
    /// This function does most of the work to construct the built wheel. It will
    /// derive special files like `.dist-info/WHEEL` and `.dist-info/RECORD` and
    /// join them with files already registered in the builder.
    pub fn build_file_manifest(&self) -> Result<FileManifest> {
        let mut m = self.manifest.clone();

        // Add the .dist-info/WHEEL file if it hasn't been provided already.
        if !m.has_path(self.dist_info_path().join("WHEEL")) {
            m.add_file_entry(
                self.dist_info_path().join("WHEEL"),
                self.derive_wheel_file().as_bytes(),
            )?;
        }

        // Add the .dist-info/METADATA file if it hasn't been provided already.
        if !m.has_path(self.dist_info_path().join("METADATA")) {
            m.add_file_entry(
                self.dist_info_path().join("METADATA"),
                self.derive_metadata_file().as_bytes(),
            )?;
        }

        // We derive the RECORD file. But it could have been added as a file. Ensure
        // it doesn't exist.
        m.remove(self.dist_info_path().join("RECORD"));

        m.add_file_entry(
            self.dist_info_path().join("RECORD"),
            self.derive_record_file(&m)
                .context("deriving RECORD file")?
                .as_bytes(),
        )?;

        Ok(m)
    }

    /// Writes the contents of a wheel file to a writable destination.
    ///
    /// Wheels are zip files. So this function effectively materializes a zip file
    /// to the specified writer.
    pub fn write_wheel_data(&self, writer: &mut (impl Write + Seek)) -> Result<()> {
        let m = self
            .build_file_manifest()
            .context("building wheel file manifest")?;

        // We place the special .dist-info/ files last, as recommended by PEP 427.
        let mut files = m.iter_files().collect::<Vec<_>>();
        let dist_info_path = self.dist_info_path();
        files.sort_by(|a, b| {
            if a.path().starts_with(&dist_info_path) && !b.path().starts_with(&dist_info_path) {
                Ordering::Greater
            } else if b.path().starts_with(&dist_info_path)
                && !a.path().starts_with(&dist_info_path)
            {
                Ordering::Less
            } else {
                a.path().cmp(b.path())
            }
        });

        let mut zf = zip::ZipWriter::new(writer);

        for file in files.into_iter() {
            let options = zip::write::FileOptions::default()
                .unix_permissions(if file.entry().is_executable() {
                    0o0755
                } else {
                    0o0644
                })
                .last_modified_time(
                    zip::DateTime::from_date_and_time(
                        self.modified_time.year() as u16,
                        self.modified_time.month() as u8,
                        self.modified_time.day(),
                        self.modified_time.hour(),
                        self.modified_time.minute(),
                        self.modified_time.second(),
                    )
                    .map_err(|_| anyhow!("could not convert time to zip::DateTime"))?,
                );

            zf.start_file(format!("{}", file.path().display()), options)?;
            zf.write_all(
                &file
                    .entry()
                    .resolve_content()
                    .with_context(|| format!("resolving content of {}", file.path().display()))?,
            )
            .with_context(|| format!("writing zip member {}", file.path().display()))?;
        }

        zf.finish().context("finishing zip file")?;

        Ok(())
    }

    /// Write the wheel file into a given directory, which must exist.
    ///
    /// Returns the path of the written wheel file on success.
    ///
    /// The wheel file isn't created until after wheel content generation. So
    /// the only scenario in which the file would exist but not have appropriate
    /// content is if some kind of I/O error occurred.
    pub fn write_wheel_into_directory(&self, directory: impl AsRef<Path>) -> Result<PathBuf> {
        let path = directory.as_ref().join(self.wheel_file_name());

        let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
        self.write_wheel_data(&mut cursor)
            .context("creating wheel zip data")?;

        std::fs::write(&path, cursor.into_inner())
            .with_context(|| format!("writing wheel data to {}", path.display()))?;

        Ok(path)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn empty() -> Result<()> {
        let builder = WheelBuilder::new("my-package", "0.1");

        let mut dest = std::io::Cursor::new(Vec::<u8>::new());
        builder.write_wheel_data(&mut dest)?;

        let m = builder.build_file_manifest()?;
        assert_eq!(m.iter_entries().count(), 3);
        assert_eq!(m.get("my_package-0.1.dist-info/WHEEL"),
                   Some(&b"Wheel-Version: 1.0\nGenerator: rust-python-packaging\nRoot-Is-Purelib: false\nTag: py3-none-any\n".as_ref().into()));
        assert_eq!(
            m.get("my_package-0.1.dist-info/METADATA"),
            Some(
                &b"Metadata-Version: 2.1\nName: my-package\nVersion: 0.1\n"
                    .as_ref()
                    .into()
            )
        );
        assert_eq!(
            m.get("my_package-0.1.dist-info/RECORD"),
            Some(&b"my_package-0.1.dist-info/METADATA,sha256=sXUNNYpfVReu7VHhVzSbKiT5ciO4Fwcwm7icBNiYn3Y,52\nmy_package-0.1.dist-info/WHEEL,sha256=76DhAzqMvlOgtCOiUNpWcD643b1CXd507uRH1hq6fQw,93\nmy_package-0.1.dist-info/RECORD,,\n".as_ref().into())
        );

        Ok(())
    }

    #[test]
    fn wheel_file_name() -> Result<()> {
        let mut builder = WheelBuilder::new("my-package", "0.1");

        assert_eq!(builder.wheel_file_name(), "my_package-0.1-py3-none-any.whl");

        builder.set_python_tag("py39");
        assert_eq!(
            builder.wheel_file_name(),
            "my_package-0.1-py39-none-any.whl"
        );

        builder.set_abi_tag("abi");
        assert_eq!(builder.wheel_file_name(), "my_package-0.1-py39-abi-any.whl");

        builder.set_platform_tag("platform");
        assert_eq!(
            builder.wheel_file_name(),
            "my_package-0.1-py39-abi-platform.whl"
        );

        builder.set_tag("py3-none-any")?;
        assert_eq!(builder.wheel_file_name(), "my_package-0.1-py3-none-any.whl");

        builder.set_build_tag("build");
        assert_eq!(
            builder.wheel_file_name(),
            "my_package-0.1-build-py3-none-any.whl"
        );

        Ok(())
    }

    #[test]
    fn custom_wheel_file() -> Result<()> {
        let mut builder = WheelBuilder::new("my-package", "0.1");

        builder.add_file_dist_info("WHEEL", vec![42])?;

        let m = builder.build_file_manifest()?;
        assert_eq!(
            m.get("my_package-0.1.dist-info/WHEEL"),
            Some(&vec![42].into())
        );

        Ok(())
    }

    #[test]
    fn custom_metadata_file() -> Result<()> {
        let mut builder = WheelBuilder::new("my-package", "0.1");

        builder.add_file_dist_info("METADATA", vec![42])?;

        let m = builder.build_file_manifest()?;
        assert_eq!(
            m.get("my_package-0.1.dist-info/METADATA"),
            Some(&vec![42].into())
        );

        Ok(())
    }

    #[test]
    fn add_file_data() -> Result<()> {
        let mut builder = WheelBuilder::new("my-package", "0.1");

        builder.add_file_data("purelib", "__init__.py", vec![42])?;

        let m = builder.build_file_manifest()?;
        assert_eq!(
            m.get("my_package-0.1.data/purelib/__init__.py"),
            Some(&vec![42].into())
        );

        Ok(())
    }
}
