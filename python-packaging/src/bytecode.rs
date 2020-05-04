// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Work with Python bytecode. */

use {
    super::resource::BytecodeOptimizationLevel,
    anyhow::{anyhow, Result},
    byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt},
    std::fs::File,
    std::io::{BufRead, BufReader, Read, Write},
    std::path::{Path, PathBuf},
    std::process,
};

pub const BYTECODE_COMPILER: &[u8] = include_bytes!("bytecodecompiler.py");

/// An entity to perform Python bytecode compilation.
#[derive(Debug)]
pub struct BytecodeCompiler {
    _temp_dir: tempdir::TempDir,
    command: process::Child,

    /// Magic number for bytecode header.
    pub magic_number: u32,
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
    pub fn new(python: &Path) -> Result<BytecodeCompiler> {
        let temp_dir = tempdir::TempDir::new("bytecode-compiler")?;

        let script_path = PathBuf::from(temp_dir.path()).join("bytecodecompiler.py");

        {
            let mut fh = File::create(&script_path)?;
            fh.write_all(BYTECODE_COMPILER)?;
        }

        let mut command = process::Command::new(python)
            .arg(script_path)
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .spawn()?;

        let stdin = command
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("unable to get stdin"))?;

        stdin.write_all(b"magic_number\n")?;
        stdin.flush()?;

        let stdout = command
            .stdout
            .as_mut()
            .ok_or_else(|| anyhow!("unable to get stdou"))?;
        let magic_number = stdout.read_u32::<LittleEndian>()?;

        Ok(BytecodeCompiler {
            _temp_dir: temp_dir,
            command,
            magic_number,
        })
    }

    /// Compile Python source into bytecode with an optimization level.
    pub fn compile(
        self: &mut BytecodeCompiler,
        source: &[u8],
        filename: &str,
        optimize: BytecodeOptimizationLevel,
        output_mode: CompileMode,
    ) -> Result<Vec<u8>> {
        let stdin = self.command.stdin.as_mut().expect("failed to get stdin");
        let stdout = self.command.stdout.as_mut().expect("failed to get stdout");

        let mut reader = BufReader::new(stdout);

        stdin.write_all(b"compile\n")?;
        stdin.write_all(filename.len().to_string().as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.write_all(source.len().to_string().as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.write_all(i32::from(optimize).to_string().as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.write_all(match output_mode {
            CompileMode::Bytecode => b"bytecode",
            CompileMode::PycCheckedHash => b"pyc-checked-hash",
            CompileMode::PycUncheckedHash => b"pyc-unchecked-hash",
        })?;
        stdin.write_all(b"\n")?;
        stdin.write_all(filename.as_bytes())?;
        stdin.write_all(source)?;
        stdin.flush()?;

        let mut len_s = String::new();
        reader.read_line(&mut len_s)?;

        let len_s = len_s.trim_end();
        let bytecode_len = len_s.parse::<u64>().unwrap();

        let mut bytecode: Vec<u8> = Vec::new();
        reader.take(bytecode_len).read_to_end(&mut bytecode)?;

        Ok(bytecode)
    }
}

impl Drop for BytecodeCompiler {
    fn drop(&mut self) {
        let stdin = self.command.stdin.as_mut().expect("failed to get stdin");
        stdin.write_all(b"exit\n").expect("write failed");
        stdin.flush().expect("flush failed");

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
