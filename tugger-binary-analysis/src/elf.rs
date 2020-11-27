// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::UndefinedSymbol,
    byteorder::ReadBytesExt,
    std::{ffi::CStr, os::raw::c_char},
};

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

                let mut reader = std::io::Cursor::new(data);

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
