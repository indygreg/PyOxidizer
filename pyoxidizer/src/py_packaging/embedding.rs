// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for embedding Python in a binary. */

use {
    crate::py_packaging::config::PyembedPythonInterpreterConfig,
    anyhow::{anyhow, Context, Result},
    pyo3_build_config::{
        BuildFlags, InterpreterConfig as PyO3InterpreterConfig, PythonImplementation, PythonVersion,
    },
    python_packaging::resource_collection::CompiledResourcesCollection,
    std::path::{Path, PathBuf},
    tugger_file_manifest::FileManifest,
};

/// Describes extra behavior for a linker invocation.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkingAnnotation {
    /// Link an Apple framework library of the given name.
    LinkFramework(String),

    /// Link a library of the given name.
    LinkLibrary(String),

    /// Link a static library of the given name.
    LinkLibraryStatic(String),

    /// A search path for libraries.
    Search(PathBuf),

    /// A search path for native libraries.
    SearchNative(PathBuf),
}

impl LinkingAnnotation {
    /// Convert the instance to a `cargo:*` string representing this annotation.
    pub fn to_cargo_annotation(&self) -> String {
        match self {
            Self::LinkFramework(framework) => {
                format!("cargo:rustc-link-lib=framework={}", framework)
            }
            Self::LinkLibrary(lib) => format!("cargo:rustc-link-lib={}", lib),
            Self::LinkLibraryStatic(lib) => format!("cargo:rustc-link-lib=static={}", lib),
            Self::Search(path) => format!("cargo:rustc-link-search={}", path.display()),
            Self::SearchNative(path) => {
                format!("cargo:rustc-link-search=native={}", path.display())
            }
        }
    }
}

/// Represents a linkable target defining a Python implementation.
pub trait LinkablePython {
    /// Write any files that need to exist to support linking.
    ///
    /// Files will be written to the directory specified.
    fn write_files(&self, dest_dir: &Path, target_triple: &str) -> Result<()>;

    /// Obtain linker annotations needed to link this libpython.
    ///
    /// `dest_dir` will be the directory where any files written by `write_files()` will
    /// be located.
    ///
    /// `alias` denotes whether to alias the library name to `pythonXY`.
    fn linking_annotations(&self, dest_dir: &Path, alias: bool) -> Result<Vec<LinkingAnnotation>>;
}

/// Link against a shared library on the filesystem.
#[derive(Clone, Debug)]
pub struct LinkSharedLibraryPath {
    /// Path to dynamic library to link.
    pub library_path: PathBuf,

    /// Additional linking annotations.
    pub linking_annotations: Vec<LinkingAnnotation>,
}

impl LinkSharedLibraryPath {
    /// Resolve the name of the library.
    fn library_name(&self) -> Result<String> {
        let filename = self
            .library_path
            .file_name()
            .ok_or_else(|| anyhow!("unable to resolve shared library file name"))?
            .to_string_lossy();

        if filename.ends_with(".dll") {
            Ok(filename.trim_end_matches(".dll").to_string())
        } else if filename.ends_with(".dylib") {
            Ok(filename
                .trim_end_matches(".dylib")
                .trim_start_matches("lib")
                .to_string())
        } else if filename.ends_with(".so") {
            Ok(filename
                .trim_end_matches(".so")
                .trim_start_matches("lib")
                .to_string())
        } else {
            Err(anyhow!(
                "unhandled libpython shared library filename: {}",
                filename
            ))
        }
    }
}

impl LinkablePython for LinkSharedLibraryPath {
    fn write_files(&self, _dest_dir: &Path, _target_triple: &str) -> Result<()> {
        Ok(())
    }

    fn linking_annotations(&self, _dest_dir: &Path, alias: bool) -> Result<Vec<LinkingAnnotation>> {
        let lib_dir = self
            .library_path
            .parent()
            .ok_or_else(|| anyhow!("could not derive parent directory of library path"))?;

        let mut annotations = vec![
            LinkingAnnotation::LinkLibrary(if alias {
                format!("pythonXY:{}", self.library_name()?)
            } else {
                self.library_name()?
            }),
            LinkingAnnotation::SearchNative(lib_dir.to_path_buf()),
        ];

        annotations.extend(self.linking_annotations.iter().cloned());

        Ok(annotations)
    }
}

/// Link against a custom built static library with tracked library data.
#[derive(Clone, Debug)]
pub struct LinkStaticLibraryData {
    /// libpython static library content.
    pub library_data: Vec<u8>,

    /// Additional linker directives to link this static library.
    pub linking_annotations: Vec<LinkingAnnotation>,
}

impl LinkStaticLibraryData {
    fn library_name(&self) -> &'static str {
        "python3"
    }

    fn library_path(&self, dest_dir: impl AsRef<Path>, target_triple: &str) -> PathBuf {
        dest_dir
            .as_ref()
            .join(if target_triple.contains("-windows-") {
                format!("{}.lib", self.library_name())
            } else {
                format!("lib{}.a", self.library_name())
            })
    }
}

impl LinkablePython for LinkStaticLibraryData {
    fn write_files(&self, dest_dir: &Path, target_triple: &str) -> Result<()> {
        let lib_path = self.library_path(dest_dir, target_triple);

        std::fs::write(&lib_path, &self.library_data)
            .with_context(|| format!("writing {}", lib_path.display()))?;

        Ok(())
    }

    fn linking_annotations(&self, dest_dir: &Path, alias: bool) -> Result<Vec<LinkingAnnotation>> {
        let mut annotations = vec![
            LinkingAnnotation::LinkLibraryStatic(if alias {
                format!("pythonXY:{}", self.library_name())
            } else {
                self.library_name().to_string()
            }),
            LinkingAnnotation::SearchNative(dest_dir.to_path_buf()),
        ];

        annotations.extend(self.linking_annotations.iter().cloned());

        Ok(annotations)
    }
}

/// Describes how to link a `libpython`.
pub enum LibpythonLinkSettings {
    /// Link against an existing shared library.
    ExistingDynamic(LinkSharedLibraryPath),
    /// Link against a custom static library.
    StaticData(LinkStaticLibraryData),
}

impl LinkablePython for LibpythonLinkSettings {
    fn write_files(&self, dest_dir: &Path, target_triple: &str) -> Result<()> {
        match self {
            Self::ExistingDynamic(l) => l.write_files(dest_dir, target_triple),
            Self::StaticData(l) => l.write_files(dest_dir, target_triple),
        }
    }

    fn linking_annotations(&self, dest_dir: &Path, alias: bool) -> Result<Vec<LinkingAnnotation>> {
        match self {
            Self::ExistingDynamic(l) => l.linking_annotations(dest_dir, alias),
            Self::StaticData(l) => l.linking_annotations(dest_dir, alias),
        }
    }
}

impl From<LinkSharedLibraryPath> for LibpythonLinkSettings {
    fn from(l: LinkSharedLibraryPath) -> Self {
        Self::ExistingDynamic(l)
    }
}

impl From<LinkStaticLibraryData> for LibpythonLinkSettings {
    fn from(l: LinkStaticLibraryData) -> Self {
        Self::StaticData(l)
    }
}

/// Filename of artifact containing the default PythonInterpreterConfig.
pub const DEFAULT_PYTHON_CONFIG_FILENAME: &str = "default_python_config.rs";

/// Holds context necessary to embed Python in a binary.
pub struct EmbeddedPythonContext<'a> {
    /// The configuration for the embedded interpreter.
    pub config: PyembedPythonInterpreterConfig,

    /// Information on how to link against Python.
    pub link_settings: LibpythonLinkSettings,

    /// Python resources that need to be serialized to a file.
    pub pending_resources: Vec<(CompiledResourcesCollection<'a>, PathBuf)>,

    /// Extra files to install next to produced binary.
    pub extra_files: FileManifest,

    /// Rust target triple for the host we are running on.
    pub host_triple: String,

    /// Rust target triple for the target we are building for.
    pub target_triple: String,

    /// Name of the Python implementation.
    pub python_implementation: PythonImplementation,

    /// Python interpreter version.
    pub python_version: PythonVersion,

    /// Path to a `python` executable that runs on the host/build machine.
    pub python_exe_host: PathBuf,

    /// Python build flags.
    ///
    /// To pass to PyO3.
    pub python_build_flags: BuildFlags,
}

impl<'a> EmbeddedPythonContext<'a> {
    /// Obtain the filesystem of the generated Rust source file containing the interpreter configuration.
    pub fn interpreter_config_rs_path(&self, dest_dir: impl AsRef<Path>) -> PathBuf {
        dest_dir.as_ref().join(DEFAULT_PYTHON_CONFIG_FILENAME)
    }

    /// Resolve the filesystem path to the PyO3 configuration file.
    pub fn pyo3_config_path(&self, dest_dir: impl AsRef<Path>) -> PathBuf {
        dest_dir.as_ref().join("pyo3-build-config-file.txt")
    }

    /// Resolve a [PyO3InterpreterConfig] for this instance.
    pub fn pyo3_interpreter_config(
        &self,
        dest_dir: impl AsRef<Path>,
    ) -> Result<PyO3InterpreterConfig> {
        Ok(PyO3InterpreterConfig {
            implementation: self.python_implementation,
            version: self.python_version,
            // Irrelevant since we control link settings below.
            shared: matches!(
                &self.link_settings,
                LibpythonLinkSettings::ExistingDynamic(_)
            ),
            // pyembed requires the full Python API.
            abi3: false,
            // We define linking info via explicit build script lines.
            lib_name: None,
            lib_dir: None,
            executable: Some(self.python_exe_host.to_string_lossy().to_string()),
            // TODO set from Python distribution metadata.
            pointer_width: Some(if self.target_triple.starts_with("i686-") {
                32
            } else {
                64
            }),
            build_flags: BuildFlags(self.python_build_flags.0.clone()),
            suppress_build_script_link_lines: true,
            extra_build_script_lines: self
                .link_settings
                .linking_annotations(dest_dir.as_ref(), self.target_triple.contains("-windows-"))?
                .iter()
                .map(|la| la.to_cargo_annotation())
                .collect::<Vec<_>>(),
        })
    }

    /// Ensure packed resources files are written.
    pub fn write_packed_resources(&self, dest_dir: impl AsRef<Path>) -> Result<()> {
        for (collection, path) in &self.pending_resources {
            let dest_path = dest_dir.as_ref().join(path);

            let mut writer = std::io::BufWriter::new(
                std::fs::File::create(&dest_path)
                    .with_context(|| format!("opening {} for writing", dest_path.display()))?,
            );
            collection
                .write_packed_resources(&mut writer)
                .context("writing packed resources")?;
        }

        Ok(())
    }

    /// Ensure files required by libpython are written.
    pub fn write_libpython(&self, dest_dir: impl AsRef<Path>) -> Result<()> {
        self.link_settings
            .write_files(dest_dir.as_ref(), &self.target_triple)
    }

    /// Write the file containing the default interpreter configuration Rust struct.
    pub fn write_interpreter_config_rs(&self, dest_dir: impl AsRef<Path>) -> Result<()> {
        self.config
            .write_default_python_config_rs(self.interpreter_config_rs_path(&dest_dir))?;

        Ok(())
    }

    /// Write the PyO3 configuration file.
    pub fn write_pyo3_config(&self, dest_dir: impl AsRef<Path>) -> Result<()> {
        let dest_dir = dest_dir.as_ref();

        let mut fh = std::fs::File::create(self.pyo3_config_path(dest_dir))?;
        self.pyo3_interpreter_config(dest_dir)?
            .to_writer(&mut fh)
            .map_err(|e| anyhow!("error writing PyO3 config file: {}", e))?;

        Ok(())
    }

    /// Write an archive containing extra files.
    pub fn write_extra_files(&self, dest_dir: impl AsRef<Path>) -> Result<()> {
        let mut fh = std::fs::File::create(dest_dir.as_ref().join("extra_files.tar.zstd"))?;
        let mut ar = tar::Builder::new(zstd::Encoder::new(fh, 0)?);
        ar.mode(tar::HeaderMode::Deterministic);
        for (path, e) in self.extra_files.iter_entries() {
            if let Some(src) = e.file_data().backing_path() {
                ar.append_path_with_name(src, path)?;
                continue;
            }
            let b = e.resolve_content()?;
            let mut h = tar::Header::new_gnu();
            h.set_size(b.len() as u64);
            h.set_mode(if e.is_executable() { 0o755 } else { 0o644 });
            h.set_uid(0);
            h.set_gid(0);
            h.set_mtime(1153704088);
            ar.append_data(&mut h, path, b.as_slice())?;
        }
        Ok(ar.into_inner()?.finish()?.sync_data()?)
    }

    /// Write out files needed to build a binary against our configuration.
    pub fn write_files(&self, dest_dir: &Path) -> Result<()> {
        self.write_packed_resources(&dest_dir)
            .context("write_packed_resources()")?;
        self.write_libpython(&dest_dir)
            .context("write_libpython()")?;
        self.write_interpreter_config_rs(&dest_dir)
            .context("write_interpreter_config_rs()")?;
        self.write_pyo3_config(&dest_dir)
            .context("write_pyo3_config()")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dynamic_library_name() -> Result<()> {
        assert_eq!(
            LinkSharedLibraryPath {
                library_path: "libpython3.9.so".into(),
                linking_annotations: vec![],
            }
            .library_name()?,
            "python3.9"
        );

        assert_eq!(
            LinkSharedLibraryPath {
                library_path: "libpython3.9.dylib".into(),
                linking_annotations: vec![],
            }
            .library_name()?,
            "python3.9"
        );

        assert_eq!(
            LinkSharedLibraryPath {
                library_path: "python3.dll".into(),
                linking_annotations: vec![],
            }
            .library_name()?,
            "python3"
        );

        Ok(())
    }
}
