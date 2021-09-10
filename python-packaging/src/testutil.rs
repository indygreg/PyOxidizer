// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        bytecode::{CompileMode, PythonBytecodeCompiler},
        resource::BytecodeOptimizationLevel,
    },
    anyhow::Result,
};

pub struct FakeBytecodeCompiler {
    pub magic_number: u32,
}

impl PythonBytecodeCompiler for FakeBytecodeCompiler {
    fn get_magic_number(&self) -> u32 {
        self.magic_number
    }

    fn compile(
        &mut self,
        source: &[u8],
        _filename: &str,
        optimize: BytecodeOptimizationLevel,
        _output_mode: CompileMode,
    ) -> Result<Vec<u8>> {
        let mut res = Vec::new();

        res.extend(match optimize {
            BytecodeOptimizationLevel::Zero => b"bc0",
            BytecodeOptimizationLevel::One => b"bc1",
            BytecodeOptimizationLevel::Two => b"bc2",
        });

        res.extend(source);

        Ok(res)
    }
}
