// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use object::{
    write, Object, ObjectSection, RelocationTarget, SectionKind, SymbolFlags, SymbolKind,
    SymbolSection,
};
use slog::{info, warn};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone)]
pub struct NoRewriteError;

impl fmt::Display for NoRewriteError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "no object rewriting was performed")
    }
}

impl Error for NoRewriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

/// Rename object syn PyInit_foo to PyInit_<full_name> to avoid clashes
pub fn rename_init(
    logger: &slog::Logger,
    name: &String,
    object_data: &[u8],
) -> Result<Vec<u8>, NoRewriteError> {
    let mut rewritten = false;

    let name_prefix = name.split('.').next().unwrap();

    let in_object = match object::File::parse(object_data) {
        Ok(object) => object,
        Err(err) => {
            let magic = [
                object_data[0],
                object_data[1],
                object_data[2],
                object_data[3],
            ];
            warn!(
                logger,
                "Failed to parse compiled object for {} (magic {:x?}): {}", name, magic, err
            );
            return Err(NoRewriteError);
        }
    };

    let mut out_object = write::Object::new(in_object.format(), in_object.architecture());
    out_object.flags = in_object.flags();

    let mut out_sections = HashMap::new();
    for in_section in in_object.sections() {
        if in_section.kind() == SectionKind::Metadata {
            continue;
        }
        let section_id = out_object.add_section(
            in_section.segment_name().unwrap_or("").as_bytes().to_vec(),
            in_section.name().unwrap_or("").as_bytes().to_vec(),
            in_section.kind(),
        );
        let out_section = out_object.section_mut(section_id);
        if out_section.is_bss() {
            out_section.append_bss(in_section.size(), in_section.align());
        } else {
            out_section.set_data(in_section.uncompressed_data().into(), in_section.align());
        }
        out_section.flags = in_section.flags();
        out_sections.insert(in_section.index(), section_id);
    }

    let mut out_symbols = HashMap::new();
    for (symbol_index, in_symbol) in in_object.symbols() {
        if in_symbol.kind() == SymbolKind::Null {
            // This is normal in ELF
            info!(logger, "object symbol name kind 'null' discarded",);
            continue;
        }
        let in_sym_name = in_symbol.name().unwrap_or("");
        if in_symbol.kind() == SymbolKind::Unknown {
            warn!(
                logger,
                "object symbol name {} kind 'unknown' encountered", in_sym_name,
            );
        }
        let (section, value) = match in_symbol.section() {
            SymbolSection::Unknown => panic!("unknown symbol section for {:?}", in_symbol),
            SymbolSection::Undefined => (write::SymbolSection::Undefined, in_symbol.address()),
            SymbolSection::Absolute => (write::SymbolSection::Absolute, in_symbol.address()),
            SymbolSection::Common => (write::SymbolSection::Common, in_symbol.address()),
            SymbolSection::Section(index) => (
                write::SymbolSection::Section(*out_sections.get(&index).unwrap()),
                in_symbol.address() - in_object.section_by_index(index).unwrap().address(),
            ),
        };
        let flags = match in_symbol.flags() {
            SymbolFlags::None => SymbolFlags::None,
            SymbolFlags::Elf { st_info, st_other } => SymbolFlags::Elf { st_info, st_other },
            SymbolFlags::MachO { n_desc } => SymbolFlags::MachO { n_desc },
            SymbolFlags::CoffSection {
                selection,
                associative_section,
            } => {
                let associative_section = *out_sections.get(&associative_section).unwrap();
                SymbolFlags::CoffSection {
                    selection,
                    associative_section,
                }
            }
        };
        let sym_name = if !in_sym_name.starts_with("$")
            && in_sym_name.contains("PyInit_")
            && !in_sym_name.contains(name_prefix)
        {
            "PyInit_".to_string() + &name.replace(".", "_")
        } else {
            String::from(in_sym_name)
        };
        if sym_name != in_sym_name {
            warn!(
                logger,
                "renaming object symbol name {} to {}", in_sym_name, sym_name,
            );

            rewritten = true;
        }

        let out_symbol = write::Symbol {
            name: sym_name.as_bytes().to_vec(),
            value,
            size: in_symbol.size(),
            kind: in_symbol.kind(),
            scope: in_symbol.scope(),
            weak: in_symbol.is_weak(),
            section,
            flags,
        };

        let symbol_id = out_object.add_symbol(out_symbol);
        out_symbols.insert(symbol_index, symbol_id);
        info!(
            logger,
            "added object symbol name {} kind {:?}", sym_name, in_symbol,
        );
    }

    if !rewritten {
        warn!(logger, "no symbol name rewriting occurred for {}", name);
        return Err(NoRewriteError);
    }

    for in_section in in_object.sections() {
        if in_section.kind() == SectionKind::Metadata {
            continue;
        }
        let out_section = *out_sections.get(&in_section.index()).unwrap();
        for (offset, in_relocation) in in_section.relocations() {
            let symbol = match in_relocation.target() {
                RelocationTarget::Symbol(symbol) => *out_symbols.get(&symbol).unwrap(),
                RelocationTarget::Section(section) => {
                    out_object.section_symbol(*out_sections.get(&section).unwrap())
                }
            };
            let out_relocation = write::Relocation {
                offset,
                size: in_relocation.size(),
                kind: in_relocation.kind(),
                encoding: in_relocation.encoding(),
                symbol,
                addend: in_relocation.addend(),
            };
            out_object
                .add_relocation(out_section, out_relocation)
                .unwrap();
        }
    }

    info!(logger, "serialising object for {} ..", name);

    match out_object.write() {
        Ok(obj) => Ok(obj),
        Err(err) => {
            warn!(logger, "object {} serialisation failed: {}", name, err);

            Err(NoRewriteError)
        }
    }
}
