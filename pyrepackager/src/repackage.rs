// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use byteorder::{LittleEndian, WriteBytesExt};
use std::env;
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::PathBuf;

use super::bytecode::compile_bytecode;
use super::dist::PythonDistributionInfo;

pub const PYTHON_IMPORTER: &'static [u8] = include_bytes!("memoryimporter.py");

pub struct ImportlibData {
    pub bootstrap_source: Vec<u8>,
    pub bootstrap_bytecode: Vec<u8>,
    pub bootstrap_external_source: Vec<u8>,
    pub bootstrap_external_bytecode: Vec<u8>,
}

/// Produce frozen importlib bytecode data.
///
/// importlib._bootstrap isn't modified.
///
/// importlib._bootstrap_external is modified. We take the original Python
/// source and concatenate with code that provides the memory importer.
/// Bytecode is then derived from it.
pub fn derive_importlib(dist: &PythonDistributionInfo) -> ImportlibData {
    let mod_bootstrap = dist.py_modules.get("importlib._bootstrap").unwrap();
    let mod_bootstrap_external = dist.py_modules.get("importlib._bootstrap_external").unwrap();

    let bootstrap_source = &mod_bootstrap.py;
    let module_name = "<frozen importlib._bootstrap>";
    let bootstrap_bytecode = compile_bytecode(bootstrap_source, module_name, 0);

    let mut bootstrap_external_source = mod_bootstrap_external.py.clone();
    bootstrap_external_source.extend("\n# END OF importlib/_bootstrap_external.py\n\n".bytes());
    bootstrap_external_source.extend(PYTHON_IMPORTER);
    let module_name = "<frozen importlib._bootstrap_external>";
    let bootstrap_external_bytecode = compile_bytecode(&bootstrap_external_source, module_name, 0);

    ImportlibData {
        bootstrap_source: bootstrap_source.clone(),
        bootstrap_bytecode,
        bootstrap_external_source: bootstrap_external_source.clone(),
        bootstrap_external_bytecode,
    }
}

/// Represents a resource entry. Simple a name-value pair.
pub struct BlobEntry {
    pub name: String,
    pub data: Vec<u8>,
}

/// Represents an ordered collection of resource entries.
pub type BlobEntries = Vec<BlobEntry>;

/// Serialize a BlobEntries to a writer.
///
/// Format:
///    Little endian u32 total number of entries.
///    Array of 2-tuples of
///        Little endian u32 length of entity name
///        Little endian u32 length of entity value
///    Vector of entity names, with no padding
///    Vector of entity values, with no padding
///
/// The "index" data is self-contained in the beginning of the data structure
/// to allow a linear read of a contiguous memory region in order to load
/// the index.
pub fn write_blob_entries<W: Write>(mut dest: W, entries: &BlobEntries) -> std::io::Result<()> {
    dest.write_u32::<LittleEndian>(entries.len() as u32)?;

    for entry in entries.iter() {
        let name_bytes = entry.name.as_bytes();
        dest.write_u32::<LittleEndian>(name_bytes.len() as u32)?;
        dest.write_u32::<LittleEndian>(entry.data.len() as u32)?;
    }

    for entry in entries.iter() {
        let name_bytes = entry.name.as_bytes();
        dest.write(name_bytes)?;
    }

    for entry in entries.iter() {
        dest.write(entry.data.as_slice())?;
    }

    Ok(())
}

/// Create a static libpython from a Python distribution.
pub fn link_libpython(dist: &PythonDistributionInfo) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let temp_dir = tempdir::TempDir::new("libpython").unwrap();
    let temp_dir_path = temp_dir.path();

    let mut build = cc::Build::new();

    for (obj_path, data) in &dist.objs_core {
        let parent = temp_dir_path.join(obj_path.parent().unwrap());
        create_dir_all(parent).unwrap();

        let full = temp_dir_path.join(obj_path);

        let mut fh = File::create(&full).unwrap();
        fh.write_all(data).unwrap();

        build.object(&full);
    }

    for (obj_path, data) in &dist.objs_modules {
        let parent = temp_dir_path.join(obj_path.parent().unwrap());
        create_dir_all(parent).unwrap();

        let full = temp_dir_path.join(obj_path);

        let mut fh = File::create(&full).unwrap();
        fh.write_all(data).unwrap();

        build.object(&full);
    }

    // Extract and link against libraries.
    for (library, data) in &dist.libraries {
        let library_filename = format!("lib{}.a", library);

        let library_path = out_dir.join(library_filename);

        let mut fh = File::create(&library_path).unwrap();
        fh.write_all(data).unwrap();

        println!("cargo:rustc-link-lib=static={}", library);
    }

    build.compile("pyembedded");
}
