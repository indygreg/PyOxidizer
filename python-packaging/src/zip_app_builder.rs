// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! Functionality for building .zip file based Python applications. */

use {
    crate::{
        bytecode::{CompileMode, PythonBytecodeCompiler},
        module_util::resolve_path_for_module,
        resource::{BytecodeOptimizationLevel, PythonModuleBytecode, PythonModuleSource},
    },
    anyhow::{anyhow, Context, Result},
    simple_file_manifest::{set_executable, FileEntry, FileManifest},
    std::{
        io::{Seek, Write},
        path::Path,
    },
    zip::CompressionMethod,
};

/// Interface for building .zip file based Python applications.
///
/// This type implements functionality provided by the Python stdlib `zipapp`
/// module. It is used to produce zip files containing Python resources
/// (notably module source and bytecode) that Python interpreters can execute
/// as standalone applications.
///
/// The zip archives can contain a shebang line (`#!<interpreter>`) denoting
/// a program to use to execute the zipapp. This is typically `python` or some
/// such variant.
pub struct ZipAppBuilder {
    /// Interpreter to use in shebang line.
    interpreter: Option<String>,

    /// Files to store in the zip archive.
    manifest: FileManifest,

    /// Compression method to use within archive.
    compression_method: CompressionMethod,

    /// The modified time to write for files in the zip archive.
    modified_time: time::OffsetDateTime,

    /// Bytecode compiler to use for generating bytecode from Python source code.
    compiler: Option<Box<dyn PythonBytecodeCompiler>>,

    /// Optimization level for Python bytecode.
    optimize_level: BytecodeOptimizationLevel,
}

impl Default for ZipAppBuilder {
    fn default() -> Self {
        Self {
            interpreter: None,
            manifest: FileManifest::default(),
            compression_method: CompressionMethod::Stored,
            modified_time: time::OffsetDateTime::now_utc(),
            compiler: None,
            optimize_level: BytecodeOptimizationLevel::Zero,
        }
    }
}

impl ZipAppBuilder {
    /// Obtain the interpreter to use in the shebang line.
    pub fn interpreter(&self) -> Option<&str> {
        self.interpreter.as_deref()
    }

    /// Set the interpreter to use in the shebang line.
    pub fn set_interpreter(&mut self, v: impl ToString) {
        self.interpreter = Some(v.to_string());
    }

    /// Obtain the modified time for files in the wheel archive.
    pub fn modified_time(&self) -> time::OffsetDateTime {
        self.modified_time
    }

    /// Set the modified time for files in the wheel archive.
    pub fn set_modified_time(&mut self, v: time::OffsetDateTime) {
        self.modified_time = v;
    }

    /// Set the Python bytecode compiler to use to turn source code into bytecode.
    pub fn set_bytecode_compiler(&mut self, v: Box<dyn PythonBytecodeCompiler>) {
        self.compiler = Some(v);
    }

    /// Obtain the bytecode optimization level used when generating Python bytecode.
    pub fn optimize_level(&self) -> BytecodeOptimizationLevel {
        self.optimize_level
    }

    /// Set the bytecode optimization level used when generating Python bytecode.
    pub fn set_optimize_level(&mut self, v: BytecodeOptimizationLevel) {
        self.optimize_level = v;
    }

    /// Add a file to the zip archive.
    ///
    /// This is the lowest level mechanism to add an entry to the zip archive. The
    /// path/file will be added without modification.
    pub fn add_file_entry(
        &mut self,
        path: impl AsRef<Path>,
        entry: impl Into<FileEntry>,
    ) -> Result<()> {
        Ok(self.manifest.add_file_entry(path, entry)?)
    }

    /// Add Python module source code to the archive.
    ///
    /// This only adds source code, not bytecode.
    pub fn add_python_module_source(
        &mut self,
        source: &PythonModuleSource,
        prefix: &str,
    ) -> Result<()> {
        let path = source.resolve_path(prefix);

        self.manifest
            .add_file_entry(path, FileEntry::new_from_data(source.source.clone(), false))?;

        Ok(())
    }

    /// Add Python module source and corresponding bytecode to the archive.
    ///
    /// This will automatically compile bytecode at the specified optimization level
    /// given the source code provided.
    pub fn add_python_module_source_and_bytecode(
        &mut self,
        source: &PythonModuleSource,
        prefix: &str,
    ) -> Result<()> {
        let compiler = self
            .compiler
            .as_mut()
            .ok_or_else(|| anyhow!("bytecode compiler not available"))?;

        let py_path = source.resolve_path(prefix);

        // The zip-based importer doesn't use the standard __pycache__ path layout when
        // searching for .pyc files. Rather, the old Python 2 layout of searching for
        // a .pyc file in the same directory that the .py would be in is used.
        let pyc_path = py_path.with_extension("pyc");

        let bytecode = source
            .as_bytecode_module(self.optimize_level)
            .compile(compiler.as_mut(), CompileMode::PycUncheckedHash)?;

        self.manifest.add_file_entry(
            py_path,
            FileEntry::new_from_data(source.source.clone(), false),
        )?;
        self.manifest
            .add_file_entry(pyc_path, FileEntry::new_from_data(bytecode, false))?;

        Ok(())
    }

    /// Add Python module bytecode, without corresponding source code.
    pub fn add_python_module_bytecode(
        &mut self,
        bytecode: &PythonModuleBytecode,
        prefix: &str,
    ) -> Result<()> {
        // The path to bytecode in zip archives isn't the same as the typical filesystem
        // layout so we have to compute as if this is a .py file then change the extension.
        let path = resolve_path_for_module(prefix, &bytecode.name, bytecode.is_package, None)
            .with_extension("pyc");

        self.manifest.add_file_entry(
            path,
            FileEntry::new_from_data(bytecode.resolve_bytecode()?, false),
        )?;

        Ok(())
    }

    /// Define the function called when the zip-based application is executed.
    ///
    /// This defines a `__main__.py[c]` that invokes the `func` function in the `module` module.
    pub fn add_main(&mut self, module: &str, func: &str, prefix: &str) -> Result<()> {
        let source = format!(
            "# -*- coding: utf-8 -*-\nimport {}\n{}.{}()\n",
            module, module, func
        );

        let module = PythonModuleSource {
            name: "__main__".to_string(),
            source: source.as_bytes().into(),
            is_package: false,
            cache_tag: "".to_string(),
            is_stdlib: false,
            is_test: false,
        };

        if self.compiler.is_some() {
            self.add_python_module_source_and_bytecode(&module, prefix)?;
        } else {
            self.add_python_module_source(&module, prefix)?;
        }

        Ok(())
    }

    /// Writes zip archive data to a writer.
    ///
    /// This will emit a zip archive + optional leading shebang so it is runnable
    /// as a standalone executable file.
    pub fn write_zip_app(&self, writer: &mut (impl Write + Seek)) -> Result<()> {
        if let Some(interpreter) = &self.interpreter {
            writer.write_all(format!("#!{}\n", interpreter).as_bytes())?;
        }

        self.write_zip_data(writer)?;

        Ok(())
    }

    /// Write the zip archive to a filesystem path.
    pub fn write_to_path(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("creating parent directory")?;
        }

        let mut fh = std::fs::File::create(path).context("opening zip file")?;
        self.write_zip_app(&mut fh).context("writing zip file")?;
        set_executable(&mut fh).context("marking zip file as executable")?;

        Ok(())
    }

    /// Writes zip archive data to a writer.
    fn write_zip_data(&self, writer: &mut (impl Write + Seek)) -> Result<()> {
        let mut zf = zip::ZipWriter::new(writer);

        for file in self.manifest.iter_files() {
            let options = zip::write::FileOptions::default()
                .compression_method(self.compression_method)
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
}

#[cfg(test)]
mod test {
    use {super::*, crate::testutil::FakeBytecodeCompiler, std::io::Read};

    #[test]
    fn empty() -> Result<()> {
        let builder = ZipAppBuilder::default();
        let mut dest = std::io::Cursor::new(Vec::<u8>::new());
        builder.write_zip_app(&mut dest)?;

        let z = zip::ZipArchive::new(dest)?;
        assert_eq!(z.len(), 0);

        Ok(())
    }

    #[test]
    fn shebang() -> Result<()> {
        let mut builder = ZipAppBuilder::default();
        builder.set_interpreter("python");
        let mut dest = std::io::Cursor::new(Vec::<u8>::new());
        builder.write_zip_app(&mut dest)?;

        assert!(dest.get_ref().starts_with(b"#!python\n"));

        let z = zip::ZipArchive::new(dest)?;
        assert_eq!(z.len(), 0);

        Ok(())
    }

    #[test]
    fn add_source() -> Result<()> {
        let mut builder = ZipAppBuilder::default();
        builder.add_python_module_source(
            &PythonModuleSource {
                name: "foo".to_string(),
                source: b"foo".to_vec().into(),
                is_package: false,
                cache_tag: "".to_string(),
                is_stdlib: false,
                is_test: false,
            },
            "",
        )?;

        let mut dest = std::io::Cursor::new(Vec::<u8>::new());
        builder.write_zip_app(&mut dest)?;

        let mut z = zip::ZipArchive::new(dest)?;
        assert_eq!(z.len(), 1);

        let mut zf = z.by_index(0)?;
        let mut b = Vec::<u8>::new();
        zf.read_to_end(&mut b)?;
        assert_eq!(zf.name(), "foo.py");
        assert_eq!(zf.compression(), CompressionMethod::Stored);
        assert!(zf.is_file());
        assert_eq!(b, b"foo");

        Ok(())
    }

    #[test]
    fn add_source_and_bytecode_no_compiler() -> Result<()> {
        let mut builder = ZipAppBuilder::default();

        assert!(builder
            .add_python_module_source_and_bytecode(
                &PythonModuleSource {
                    name: "".to_string(),
                    source: b"".to_vec().into(),
                    is_package: false,
                    cache_tag: "".to_string(),
                    is_stdlib: false,
                    is_test: false
                },
                ""
            )
            .is_err());

        Ok(())
    }

    #[test]
    fn add_source_and_bytecode() -> Result<()> {
        let mut builder = ZipAppBuilder::default();
        builder.set_bytecode_compiler(Box::new(FakeBytecodeCompiler { magic_number: 42 }));

        let m = PythonModuleSource {
            name: "foo".to_string(),
            source: b"foo".to_vec().into(),
            is_package: false,
            cache_tag: "".to_string(),
            is_stdlib: false,
            is_test: false,
        };

        builder.add_python_module_source_and_bytecode(&m, "lib")?;

        let mut dest = std::io::Cursor::new(Vec::<u8>::new());
        builder.write_zip_app(&mut dest)?;

        let mut z = zip::ZipArchive::new(dest)?;
        assert_eq!(z.len(), 2);

        {
            let mut zf = z.by_index(0)?;
            let mut b = Vec::<u8>::new();
            zf.read_to_end(&mut b)?;
            assert_eq!(zf.name(), "lib/foo.py");
            assert_eq!(zf.compression(), CompressionMethod::Stored);
            assert!(zf.is_file());
            assert_eq!(b, m.source.resolve_content()?);
        }

        {
            let mut zf = z.by_index(1)?;
            let mut b = Vec::<u8>::new();
            zf.read_to_end(&mut b)?;
            assert_eq!(zf.name(), "lib/foo.pyc");
            assert_eq!(zf.compression(), CompressionMethod::Stored);
            assert!(zf.is_file());
            assert_eq!(b, b"bc0foo");
        }

        Ok(())
    }

    #[test]
    fn add_main() -> Result<()> {
        let mut builder = ZipAppBuilder::default();
        builder.add_main("foo", "bar", "lib")?;

        let mut dest = std::io::Cursor::new(Vec::<u8>::new());
        builder.write_zip_app(&mut dest)?;

        let mut z = zip::ZipArchive::new(dest)?;
        assert_eq!(z.len(), 1);

        let mut zf = z.by_index(0)?;
        let mut b = Vec::<u8>::new();
        zf.read_to_end(&mut b)?;
        assert_eq!(zf.name(), "lib/__main__.py");
        assert_eq!(zf.compression(), CompressionMethod::Stored);
        assert!(zf.is_file());

        Ok(())
    }
}
