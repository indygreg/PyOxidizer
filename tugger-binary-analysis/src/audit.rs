// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Analyze binaries for distribution compatibility.

use {
    crate::{
        find_minimum_distro_version, find_undefined_elf_symbols, UndefinedSymbol,
        GCC_VERSIONS_BY_DISTRO, GLIBC_VERSIONS_BY_DISTRO, LSB_SHARED_LIBRARIES,
    },
    std::{collections::BTreeMap, fs::File, io::Read, path::PathBuf},
};

pub fn analyze_file(path: PathBuf) {
    let mut fd = File::open(path).unwrap();
    let mut buffer = Vec::new();
    fd.read_to_end(&mut buffer).unwrap();
    analyze_data(&buffer);
}

pub fn analyze_data(buffer: &[u8]) {
    match goblin::Object::parse(buffer).unwrap() {
        goblin::Object::Elf(elf) => {
            let mut undefined_symbols = find_undefined_elf_symbols(&buffer, &elf);
            undefined_symbols.sort();

            analyze_elf_libraries(&elf.libraries, &undefined_symbols);
        }
        goblin::Object::PE(_pe) => {
            panic!("PE not yet supported");
        }
        goblin::Object::Mach(_mach) => {
            panic!("mach not yet supported");
        }
        goblin::Object::Archive(_archive) => {
            panic!("archive not yet supported");
        }
        goblin::Object::Unknown(magic) => panic!("unknown magic: {:#x}", magic),
    }
}

pub fn analyze_elf_libraries(libs: &[&str], undefined_symbols: &[UndefinedSymbol]) {
    let mut latest_symbols: BTreeMap<String, version_compare::Version> = BTreeMap::new();

    println!("Shared Library Dependencies");
    println!("===========================");

    let mut libs = libs.to_vec();
    libs.sort_unstable();
    for lib in libs {
        println!("{}", lib);

        if LSB_SHARED_LIBRARIES.contains(&lib) {
            println!("  OK - Library part of Linux Standard Base and present on most distros");
        } else {
            println!("  PROBLEMATIC - Shared library dependency may not be on all machines");
        }

        let mut symbols: Vec<&UndefinedSymbol> = Vec::new();

        for symbol in undefined_symbols {
            if symbol.filename == Some((*lib).to_string()) {
                symbols.push(symbol);
            }
        }

        /*
        println!("");
        println!("  Symbols");
        println!("  -------");
        */

        for symbol in symbols {
            match &symbol.version {
                Some(version) => {
                    let parts: Vec<&str> = version.splitn(2, '_').collect();

                    match parts.len() {
                        1 => { /* TODO this is weird. Do something? */ }
                        2 => {
                            let v = version_compare::Version::from(parts[1])
                                .expect("unable to parse version");

                            match latest_symbols.get(parts[0]) {
                                Some(existing) => {
                                    if &v > existing {
                                        latest_symbols.insert(parts[0].to_string(), v);
                                    }
                                }
                                None => {
                                    latest_symbols.insert(parts[0].to_string(), v);
                                }
                            }
                        }
                        _ => {}
                    }

                    //println!("  {}@{}", &symbol.symbol, version)
                }
                None => {
                    //println!("  {}", &symbol.symbol)
                }
            }
        }

        println!();
    }

    println!("Symbol Versioning");
    println!("=================");

    for (name, version) in &latest_symbols {
        match name.as_str() {
            "GLIBC" => {
                println!();
                println!("glibc");
                println!("-----");
                println!();
                println!("Minimum Version: {}", version);
                println!("Minimum Distro Versions:");

                for s in find_minimum_distro_version(&version, &GLIBC_VERSIONS_BY_DISTRO) {
                    println!("  {}", s);
                }
            }
            "GCC" => {
                println!();
                println!("gcc");
                println!("-----");
                println!();
                println!("Minimum Version: {}", version);
                println!("Minimum Distro Versions:");

                for s in find_minimum_distro_version(&version, &GCC_VERSIONS_BY_DISTRO) {
                    println!("  {}", s);
                }
            }
            other => {
                println!();
                println!("{}", other);
                println!("-----");
                println!();
                println!("Minimum Version: {}", version);
                println!("Minimum Distro Versions: Unknown");
            }
        }
    }
}
