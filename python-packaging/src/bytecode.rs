// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! Work with Python bytecode. */

use {
    super::resource::BytecodeOptimizationLevel,
    anyhow::{anyhow, Context, Result},
    byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt},
    std::{
        io::{BufRead, BufReader, Read, Write},
        path::Path,
        process,
    },
};

pub const BYTECODE_COMPILER: &[u8] = include_bytes!("bytecodecompiler.py");

/// An entity that can compile Python bytecode.
pub trait PythonBytecodeCompiler {
    /// Obtain the magic number to use in the bytecode header.
    fn get_magic_number(&self) -> u32;

    /// Compile Python source into bytecode with an optimization level.
    fn compile(
        &mut self,
        source: &[u8],
        filename: &str,
        optimize: BytecodeOptimizationLevel,
        output_mode: CompileMode,
    ) -> Result<Vec<u8>>;
}

/// An entity to perform Python bytecode compilation.
#[derive(Debug)]
pub struct BytecodeCompiler {
    command: process::Child,

    /// Magic number for bytecode header.
    magic_number: u32,
}

/// Output mode for BytecodeCompiler.
pub enum CompileMode {
    /// Emit just Python bytecode.
    Bytecode,
    /// Emit .pyc header with hash verification.
    PycCheckedHash,
    /// Emit .pyc header with no hash verification.
    PycUncheckedHash,
}

impl BytecodeCompiler {
    /// Create a bytecode compiler using a Python executable.
    ///
    /// A Python process will be started and it will start executing a Python
    /// source file embedded in this crate. That process interacts with this
    /// object via a pipe, which is used to send bytecode compilation
    /// requests and receive the compiled bytecode. The process is terminated
    /// when this object is dropped.
    ///
    /// A Python script is written to the directory passed. This should ideally be
    /// a temporary directory. The file name is deterministic, so it isn't safe
    /// for multiple callers to simultaneously pass the same directory. The temporary
    /// file is deleted before this function returns. Ideally this function would use
    /// a proper temporary file internally. The reason this isn't done is to avoid
    /// an extra crate dependency.
    pub fn new(python: &Path, script_dir: impl AsRef<Path>) -> Result<BytecodeCompiler> {
        let script_path = script_dir.as_ref().join("bytecode-compiler.py");
        std::fs::write(&script_path, BYTECODE_COMPILER)
            .with_context(|| format!("writing Python script to {}", script_path.display()))?;

        let mut command = process::Command::new(python)
            .arg(&script_path)
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .spawn()
            .with_context(|| format!("spawning {}", python.display()))?;

        let stdin = command
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("unable to get stdin"))
            .with_context(|| format!("obtaining stdin from {} process", python.display()))?;

        stdin.write_all(b"magic_number\n").with_context(|| {
            format!(
                "writing magic_number command request to {} process",
                python.display()
            )
        })?;
        stdin
            .flush()
            .with_context(|| format!("flushing stdin to {} process", python.display()))?;

        let stdout = command
            .stdout
            .as_mut()
            .ok_or_else(|| anyhow!("unable to get stdout"))?;
        let magic_number = stdout.read_u32::<LittleEndian>().with_context(|| {
            format!(
                "reading magic number from invoked {} process",
                python.display()
            )
        })?;

        std::fs::remove_file(&script_path)
            .with_context(|| format!("deleting {}", script_path.display()))?;

        Ok(BytecodeCompiler {
            command,
            magic_number,
        })
    }
}

impl PythonBytecodeCompiler for BytecodeCompiler {
    fn get_magic_number(&self) -> u32 {
        self.magic_number
    }

    fn compile(
        self: &mut BytecodeCompiler,
        source: &[u8],
        filename: &str,
        optimize: BytecodeOptimizationLevel,
        output_mode: CompileMode,
    ) -> Result<Vec<u8>> {
        let stdin = self.command.stdin.as_mut().expect("failed to get stdin");
        let stdout = self.command.stdout.as_mut().expect("failed to get stdout");

        let mut reader = BufReader::new(stdout);

        stdin
            .write_all(b"compile\n")
            .context("writing compile command")?;
        stdin
            .write_all(filename.len().to_string().as_bytes())
            .context("writing filename length")?;
        stdin.write_all(b"\n")?;
        stdin
            .write_all(source.len().to_string().as_bytes())
            .context("writing source code length")?;
        stdin.write_all(b"\n")?;
        stdin.write_all(i32::from(optimize).to_string().as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin
            .write_all(match output_mode {
                CompileMode::Bytecode => b"bytecode",
                CompileMode::PycCheckedHash => b"pyc-checked-hash",
                CompileMode::PycUncheckedHash => b"pyc-unchecked-hash",
            })
            .context("writing format")?;
        stdin.write_all(b"\n")?;
        stdin
            .write_all(filename.as_bytes())
            .context("writing filename")?;
        stdin.write_all(source).context("writing source code")?;
        stdin.flush().context("flushing")?;

        let mut code_s = String::new();
        reader
            .read_line(&mut code_s)
            .context("reading result code")?;
        let code_s = code_s.trim_end();
        let code = code_s.parse::<u8>().unwrap();

        match code {
            0 => {
                let mut len_s = String::new();
                reader
                    .read_line(&mut len_s)
                    .context("reading output size line")?;

                let len_s = len_s.trim_end();
                let bytecode_len = len_s.parse::<u64>().unwrap();

                let mut bytecode: Vec<u8> = Vec::new();
                reader
                    .take(bytecode_len)
                    .read_to_end(&mut bytecode)
                    .context("reading bytecode result")?;

                Ok(bytecode)
            }
            1 => {
                let mut len_s = String::new();
                reader
                    .read_line(&mut len_s)
                    .context("reading error string length line")?;

                let len_s = len_s.trim_end();
                let error_len = len_s.parse::<u64>().unwrap();

                let mut error_data = vec![];
                reader
                    .take(error_len)
                    .read_to_end(&mut error_data)
                    .context("reading error message")?;

                Err(anyhow!(
                    "compiling error: {}",
                    String::from_utf8(error_data)?
                ))
            }
            _ => Err(anyhow!(
                "unexpected result code from compile command: {}",
                code
            )),
        }
    }
}

impl Drop for BytecodeCompiler {
    fn drop(&mut self) {
        let stdin = self.command.stdin.as_mut().expect("failed to get stdin");
        let _ = stdin.write_all(b"exit\n").and_then(|()| stdin.flush());

        self.command.wait().expect("compiler process did not exit");
    }
}

/// How to write out a .pyc bytecode header.
#[derive(Debug, Clone, Copy)]
pub enum BytecodeHeaderMode {
    /// Use a file modified time plus source size.
    ModifiedTimeAndSourceSize((u32, u32)),
    /// Check the hash against the hash of a source file.
    CheckedHash(u64),
    /// Do not check the hash, but embed it anyway.
    UncheckedHash(u64),
}

/// Compute the header for a .pyc file.
pub fn compute_bytecode_header(magic_number: u32, mode: BytecodeHeaderMode) -> Result<Vec<u8>> {
    let mut header: Vec<u8> = Vec::new();

    header.write_u32::<LittleEndian>(magic_number)?;

    match mode {
        BytecodeHeaderMode::ModifiedTimeAndSourceSize((mtime, source_size)) => {
            header.write_u32::<LittleEndian>(0)?;
            header.write_u32::<LittleEndian>(mtime)?;
            header.write_u32::<LittleEndian>(source_size)?;
        }
        BytecodeHeaderMode::CheckedHash(hash) => {
            header.write_u32::<LittleEndian>(3)?;
            header.write_u64::<LittleEndian>(hash)?;
        }
        BytecodeHeaderMode::UncheckedHash(hash) => {
            header.write_u32::<LittleEndian>(1)?;
            header.write_u64::<LittleEndian>(hash)?;
        }
    }

    assert_eq!(header.len(), 16);

    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header() -> Result<()> {
        assert_eq!(
            compute_bytecode_header(
                168627541,
                BytecodeHeaderMode::ModifiedTimeAndSourceSize((5, 10))
            )?,
            b"U\r\r\n\x00\x00\x00\x00\x05\x00\x00\x00\x0a\x00\x00\x00"
        );

        assert_eq!(
            compute_bytecode_header(168627541, BytecodeHeaderMode::CheckedHash(0))?,
            b"U\r\r\n\x03\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
        );
        assert_eq!(
            compute_bytecode_header(168627541, BytecodeHeaderMode::UncheckedHash(0))?,
            b"U\r\r\n\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
        );

        Ok(())
    }
}
