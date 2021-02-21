// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {anyhow::Result, std::path::Path};
use rayon::prelude::*;

pub fn find_pe_dependencies(data: &[u8]) -> Result<Vec<String>> {
    let pe = goblin::pe::PE::parse(data)?;
    Ok(pe.libraries.par_iter().map(|l| (*l).to_string()).collect())
}

#[allow(unused)]
pub fn find_pe_dependencies_path(path: &Path) -> Result<Vec<String>> {
    let data = std::fs::read(path)?;
    find_pe_dependencies(&data)
}
