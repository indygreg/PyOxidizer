// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process;

pub const BYTECODE_COMPILER: &[u8] = include_bytes!("bytecodecompiler.py");

/// An entity to perform Python bytecode compilation.
pub struct BytecodeCompiler {
    _temp_dir: tempdir::TempDir,
    command: process::Child,
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
    pub fn new(python: &Path) -> BytecodeCompiler {
        let temp_dir =
            tempdir::TempDir::new("bytecode-compiler").expect("could not create temp directory");

        let script_path = PathBuf::from(temp_dir.path()).join("bytecodecompiler.py");

        {
            let mut fh = File::create(&script_path).expect("could not create temp path");
            fh.write_all(BYTECODE_COMPILER)
                .expect("could not write bytecodecompiler.py");
        }

        let command = process::Command::new(python)
            .arg(script_path.clone())
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .spawn()
            .expect("Python compiler process invoked");

        BytecodeCompiler {
            _temp_dir: temp_dir,
            command,
        }
    }

    /// Compile Python source into bytecode with an optimization level.
    pub fn compile(
        self: &mut BytecodeCompiler,
        source: &[u8],
        filename: &str,
        optimize: i32,
        output_mode: CompileMode,
    ) -> Result<Vec<u8>, std::io::Error> {
        let stdin = self.command.stdin.as_mut().expect("failed to get stdin");
        let stdout = self.command.stdout.as_mut().expect("failed to get stdout");

        let mut reader = BufReader::new(stdout);

        stdin.write_all(b"compile\n")?;
        stdin.write_all(filename.len().to_string().as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.write_all(source.len().to_string().as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.write_all(optimize.to_string().as_bytes())?;
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
