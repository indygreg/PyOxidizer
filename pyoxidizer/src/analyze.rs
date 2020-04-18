// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Analyze binaries for distribution compatibility.

use {
    anyhow::Result,
    byteorder::ReadBytesExt,
    lazy_static::lazy_static,
    std::collections::BTreeMap,
    std::ffi::CStr,
    std::fs::File,
    std::io::{Cursor, Read},
    std::os::raw::c_char,
    std::path::{Path, PathBuf},
};

const LSB_SHARED_LIBRARIES: &[&str] = &[
    "ld-linux-x86-64.so.2",
    "libc.so.6",
    "libdl.so.2",
    "libgcc_s.so.1",
    "libm.so.6",
    "libpthread.so.0",
    "librt.so.1",
    "libutil.so.1",
];

type DistroVersion = Vec<(&'static str, &'static str)>;

lazy_static! {
    static ref GLIBC_VERSIONS_BY_DISTRO: BTreeMap<&'static str, DistroVersion> = {
        let mut res: BTreeMap<&'static str, DistroVersion> = BTreeMap::new();

        let mut fedora = DistroVersion::new();
        fedora.push(("16", "2.14"));
        fedora.push(("17", "2.15"));
        fedora.push(("18", "2.16"));
        fedora.push(("19", "2.17"));
        fedora.push(("20", "2.18"));
        fedora.push(("21", "2.20"));
        fedora.push(("22", "2.21"));
        fedora.push(("23", "2.22"));
        fedora.push(("24", "2.23"));
        fedora.push(("25", "2.24"));
        fedora.push(("26", "2.25"));
        fedora.push(("27", "2.26"));
        fedora.push(("28", "2.27"));
        fedora.push(("29", "2.28"));
        res.insert("Fedora", fedora);

        let mut rhel = DistroVersion::new();
        rhel.push(("6", "2.12"));
        rhel.push(("7", "2.17"));
        res.insert("RHEL", rhel);

        let mut opensuse = DistroVersion::new();
        opensuse.push(("11.4", "2.11"));
        opensuse.push(("12.1", "2.14"));
        opensuse.push(("12.2", "2.15"));
        opensuse.push(("12.3", "2.17"));
        opensuse.push(("13.1", "2.18"));
        opensuse.push(("13.2", "2.19"));
        opensuse.push(("42.1", "2.19"));
        opensuse.push(("42.2", "2.22"));
        opensuse.push(("42.3", "2.22"));
        opensuse.push(("15.0", "2.26"));
        res.insert("OpenSUSE", opensuse);

        let mut debian = DistroVersion::new();
        debian.push(("6", "2.11"));
        debian.push(("7", "2.13"));
        debian.push(("8", "2.19"));
        debian.push(("9", "2.24"));
        res.insert("Debian", debian);

        let mut ubuntu = DistroVersion::new();
        ubuntu.push(("12.04", "2.15"));
        ubuntu.push(("14.04", "2.19"));
        ubuntu.push(("16.04", "2.23"));
        ubuntu.push(("18.04", "2.27"));
        ubuntu.push(("18.10", "2.28"));
        ubuntu.push(("19.04", "2.29"));
        res.insert("Ubuntu", ubuntu);

        res
    };
    static ref GCC_VERSIONS_BY_DISTRO: BTreeMap<&'static str, DistroVersion> = {
        let mut res: BTreeMap<&'static str, DistroVersion> = BTreeMap::new();

        let mut fedora = DistroVersion::new();
        fedora.push(("16", "4.6"));
        fedora.push(("17", "4.7"));
        fedora.push(("18", "4.7"));
        fedora.push(("19", "4.8"));
        fedora.push(("20", "4.8"));
        fedora.push(("21", "4.9"));
        fedora.push(("22", "4.9"));
        fedora.push(("23", "5.1"));
        fedora.push(("24", "6.1"));
        fedora.push(("25", "6.2"));
        fedora.push(("26", "7.1"));
        fedora.push(("27", "7.2"));
        fedora.push(("28", "8.0.1"));
        fedora.push(("29", "8.2.1"));
        res.insert("Fedora", fedora);

        let mut rhel = DistroVersion::new();
        rhel.push(("6", "4.4"));
        rhel.push(("7", "4.8"));
        res.insert("RHEL", rhel);

        let mut opensuse = DistroVersion::new();
        opensuse.push(("11.4", "4.5"));
        opensuse.push(("12.1", "4.6"));
        opensuse.push(("12.2", "4.7"));
        opensuse.push(("12.3", "4.7"));
        opensuse.push(("13.1", "4.8"));
        opensuse.push(("13.2", "4.8"));
        opensuse.push(("42.1", "4.8"));
        opensuse.push(("42.2", "4.8.5"));
        opensuse.push(("42.3", "4.8.5"));
        opensuse.push(("15.0", "7.3.1"));
        res.insert("OpenSUSE", opensuse);

        let mut debian = DistroVersion::new();
        debian.push(("6", "4.1"));
        debian.push(("7", "4.4"));
        debian.push(("8", "4.8"));
        debian.push(("9", "6.3"));
        res.insert("Debian", debian);

        let mut ubuntu = DistroVersion::new();
        ubuntu.push(("12.04", "4.4"));
        ubuntu.push(("14.04", "4.4"));
        ubuntu.push(("16.04", "4.7"));
        ubuntu.push(("18.04", "7.3"));
        res.insert("Ubuntu", ubuntu);

        res
    };
}

#[repr(C)]
#[derive(Debug, Clone)]
struct Elf64_Verdef {
    vd_version: u16,
    vd_flags: u16,
    vd_ndx: u16,
    vd_cnt: u16,
    vd_hash: u32,
    vd_aux: u32,
    vd_next: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
struct Elf64_Verneed {
    vn_version: u16,
    vn_cnt: u16,
    vn_file: u32,
    vn_aux: u32,
    vn_next: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
struct Elf64_Vernaux {
    vna_hash: u32,
    vna_flags: u16,
    vna_other: u16,
    vna_name: u32,
    vna_next: u32,
}

#[derive(Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct UndefinedSymbol {
    symbol: String,
    filename: Option<String>,
    version: Option<String>,
}

pub fn analyze_file(path: PathBuf) {
    let mut fd = File::open(path).unwrap();
    let mut buffer = Vec::new();
    fd.read_to_end(&mut buffer).unwrap();
    analyze_data(&buffer);
}

pub fn analyze_data(buffer: &[u8]) {
    match goblin::Object::parse(buffer).unwrap() {
        goblin::Object::Elf(elf) => {
            let undefined_symbols: Vec<UndefinedSymbol> =
                itertools::sorted(find_undefined_elf_symbols(&buffer, &elf).into_iter()).collect();

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

    for lib in itertools::sorted(libs) {
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

fn find_minimum_distro_version(
    version: &version_compare::Version,
    distro_versions: &BTreeMap<&'static str, DistroVersion>,
) -> Vec<String> {
    let mut res: Vec<String> = Vec::new();

    for (distro, dv) in distro_versions {
        let mut found = false;

        for (distro_version, version_version) in dv {
            let version_version = version_compare::Version::from(version_version)
                .expect("unable to parse distro version");

            if &version_version >= version {
                found = true;
                res.push(format!("{} {}", distro, distro_version));
                break;
            }
        }

        if !found {
            res.push(format!("No known {} versions supported", distro));
        }
    }

    res
}

fn resolve_verneed(
    verneed_entries: &[(Elf64_Verneed, Vec<Elf64_Vernaux>)],
    names_data: &[u8],
    versym: u16,
) -> (Option<String>, Option<String>) {
    // versym corresponds to value in Elf64_Vernaux.vna_other.
    for (verneed, vernauxes) in verneed_entries {
        for vernaux in vernauxes {
            if vernaux.vna_other != versym {
                continue;
            }

            let filename_ptr = unsafe { names_data.as_ptr().add(verneed.vn_file as usize) };
            let filename = unsafe { CStr::from_ptr(filename_ptr as *const c_char) };

            let depend_ptr = unsafe { names_data.as_ptr().add(vernaux.vna_name as usize) };
            let depend = unsafe { CStr::from_ptr(depend_ptr as *const c_char) };

            return (
                Some(filename.to_string_lossy().into_owned()),
                Some(depend.to_string_lossy().into_owned()),
            );
        }
    }

    (None, None)
}

/// Find undefined dynamic symbols in an ELF binary.
///
/// Will also resolve the filename and symbol version, if available.
#[allow(clippy::cast_ptr_alignment)]
pub fn find_undefined_elf_symbols(buffer: &[u8], elf: &goblin::elf::Elf) -> Vec<UndefinedSymbol> {
    let mut verneed_entries: Vec<(Elf64_Verneed, Vec<Elf64_Vernaux>)> = Vec::new();
    let mut versym: Vec<u16> = Vec::new();
    let mut verneed_names_section: u32 = 0;

    for section_header in &elf.section_headers {
        match section_header.sh_type {
            goblin::elf::section_header::SHT_GNU_VERSYM => {
                let data: &[u8] = &buffer[section_header.file_range()];

                let mut reader = Cursor::new(data);

                while let Ok(value) = reader.read_u16::<byteorder::NativeEndian>() {
                    versym.push(value);
                }
            }
            goblin::elf::section_header::SHT_GNU_VERNEED => {
                verneed_names_section = section_header.sh_link;

                let data: &[u8] = &buffer[section_header.file_range()];

                let mut ptr = data.as_ptr();

                for _ in 0..elf.dynamic.as_ref().unwrap().info.verneednum {
                    let record: Elf64_Verneed = unsafe { std::ptr::read(ptr as *const _) };

                    // Stash pointer to next Verneed record.
                    let next_record = unsafe { ptr.add(record.vn_next as usize) };

                    let mut vernaux: Vec<Elf64_Vernaux> = Vec::new();

                    ptr = unsafe { ptr.add(record.vn_aux as usize) };

                    for _ in 0..record.vn_cnt {
                        let aux: Elf64_Vernaux = unsafe { std::ptr::read(ptr as *const _) };
                        vernaux.push(aux.clone());
                        ptr = unsafe { ptr.add(aux.vna_next as usize) };
                    }

                    verneed_entries.push((record.clone(), vernaux));

                    ptr = next_record;
                }
            }
            _ => {}
        }
    }

    let dynstrtab = &elf.dynstrtab;
    let verneed_names_data: &[u8] =
        &buffer[elf.section_headers[verneed_names_section as usize].file_range()];

    let mut res: Vec<UndefinedSymbol> = Vec::new();

    let mut versym_iter = versym.iter();

    for sym in elf.dynsyms.iter() {
        let versym = *versym_iter.next().unwrap();

        if sym.is_import() {
            let name = dynstrtab.get(sym.st_name).unwrap().unwrap();

            res.push(if versym > 1 {
                let (filename, version) =
                    resolve_verneed(&verneed_entries, &verneed_names_data, versym);

                UndefinedSymbol {
                    symbol: String::from(name),
                    filename,
                    version,
                }
            } else {
                UndefinedSymbol {
                    symbol: String::from(name),
                    filename: None,
                    version: None,
                }
            });
        }
    }

    res
}

pub fn find_pe_dependencies(data: &[u8]) -> Result<Vec<String>> {
    let pe = goblin::pe::PE::parse(data)?;
    Ok(pe.libraries.iter().map(|l| (*l).to_string()).collect())
}

#[allow(unused)]
pub fn find_pe_dependencies_path(path: &Path) -> Result<Vec<String>> {
    let data = std::fs::read(path)?;
    find_pe_dependencies(&data)
}
