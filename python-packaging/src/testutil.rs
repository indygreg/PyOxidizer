// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

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
