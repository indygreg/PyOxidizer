// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub fn find_python_modules(root_path: &Path) -> Result<BTreeMap<String, Vec<u8>>, &'static str> {
    let mut mods = BTreeMap::new();

    for entry in walkdir::WalkDir::new(&root_path).into_iter() {
        let entry = entry.unwrap();

        let path = entry.into_path();
        let path_str = path.to_str().unwrap();

        if !path_str.ends_with(".py") {
            continue;
        }

        let rel_path = path.strip_prefix(&root_path).unwrap();

        let components = rel_path.iter().map(|p| p.to_str().unwrap()).collect::<Vec<_>>();

        let package_parts = &components[0..components.len() - 1];
        let module_name = rel_path.file_stem().unwrap().to_str().unwrap();

        let mut full_module_name: Vec<&str> = package_parts.to_vec();

        if module_name != "__init__" {
            full_module_name.push(module_name);
        }

        let full_module_name = itertools::join(full_module_name, ".");

        let mut fh = File::open(&path).unwrap();
        let mut data = Vec::new();
        fh.read_to_end(&mut data).unwrap();

        mods.insert(full_module_name, data);
    }

    Ok(mods)
}
