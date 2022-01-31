// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Binary file analysis. */

use {
    anyhow::{anyhow, Result},
    object::{
        elf,
        read::elf::{Dyn, FileHeader as ElfFileHeader, SectionHeader, Sym},
        Architecture, BinaryFormat, Endianness, FileKind, Object, ObjectKind, SectionIndex,
    },
    once_cell::sync::Lazy,
    std::{
        collections::{HashMap, HashSet},
        ops::Deref,
    },
};

pub static X86_INSTRUCTION_CODES: Lazy<HashMap<String, iced_x86::Code>> = Lazy::new(|| {
    iced_x86::Code::values()
        .map(|code| (format!("{:?}", code).to_lowercase(), code))
        .collect::<HashMap<String, iced_x86::Code>>()
});

/// Counts of unique instructions within a binary.
#[derive(Clone, Debug, Default)]
pub struct X86InstructionCounts {
    inner: HashMap<iced_x86::Instruction, u64>,
}

impl X86InstructionCounts {
    /// Obtain counts of x86 instruction codes.
    pub fn code_counts(&self) -> HashMap<iced_x86::Code, u64> {
        let mut h = HashMap::new();

        for (instruction, count) in &self.inner {
            let entry = h.entry(instruction.code()).or_default();
            *entry += count;
        }

        h
    }

    /// Obtain counts of x86 op codes.
    pub fn op_code_counts(&self) -> HashMap<u32, u64> {
        let mut h = HashMap::new();

        for (instruction, count) in &self.inner {
            let entry = h.entry(instruction.op_code().op_code()).or_default();
            *entry += count;
        }

        h
    }

    /// Obtain a set of CPUID features required by instructions in this collection.
    pub fn cpuid_features(&self) -> HashSet<&'static iced_x86::CpuidFeature> {
        HashSet::from_iter(
            self.inner
                .keys().flat_map(|instruction| instruction.cpuid_features()),
        )
    }

    /// Obtain counts of register use.
    ///
    /// If an instruction references the given register, that register's count will be
    /// incremented by the number of instruction occurrences.
    pub fn register_counts(&self) -> HashMap<iced_x86::Register, u64> {
        let mut h = HashMap::new();

        for (instruction, count) in &self.inner {
            // Registers can be referenced multiple times by operands. Only attribute the count
            // once per unique instruction.
            let registers = instruction
                .op_kinds()
                .enumerate()
                .filter(|(_, kind)| matches!(kind, iced_x86::OpKind::Register))
                .map(|(i, _)| instruction.op_register(i as _))
                .collect::<HashSet<_>>();

            for register in registers {
                let entry = h.entry(register).or_default();
                *entry += count;
            }
        }

        h
    }

    /// Obtain counts of base register use.
    ///
    /// This is similar to [Self::register_counts()] but registers are normalized to their
    /// base register. e.g. `XMM0-XMM31` are normalized to `XMM0`. This allows to see what register
    /// groups are used.
    pub fn base_register_counts(&self) -> HashMap<iced_x86::Register, u64> {
        let mut h = HashMap::new();

        for (instruction, count) in &self.inner {
            let registers = instruction
                .op_kinds()
                .enumerate()
                .filter(|(_, kind)| matches!(kind, iced_x86::OpKind::Register))
                .map(|(i, _)| instruction.op_register(i as _).base())
                .collect::<HashSet<_>>();

            for register in registers {
                let entry = h.entry(register).or_default();
                *entry += count;
            }
        }

        h
    }

    /// Obtain counts of full base register use.
    ///
    /// This is similar to [Self::register_counts()] but the register is normalized to the _full_
    /// base register. e.g. `XMM0-XMM31` are normalized to `ZMM0` since `XMM*` registers are
    /// implemented in terms of `ZMM*` registers.
    pub fn full_base_register_counts(&self) -> HashMap<iced_x86::Register, u64> {
        let mut h = HashMap::new();

        for (instruction, count) in &self.inner {
            let registers = instruction
                .op_kinds()
                .enumerate()
                .filter(|(_, kind)| matches!(kind, iced_x86::OpKind::Register))
                .map(|(i, _)| instruction.op_register(i as _).full_register().base())
                .collect::<HashSet<_>>();

            for register in registers {
                let entry = h.entry(register).or_default();
                *entry += count;
            }
        }

        h
    }
}

impl Deref for X86InstructionCounts {
    type Target = HashMap<iced_x86::Instruction, u64>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// A section within an ELF file.
#[derive(Clone, Debug)]
pub struct ElfSection {
    pub index: usize,
    pub name: String,
    pub typ: u32,
    pub flags: u64,
    pub address: u64,
    pub offset: u64,
    pub size: u64,
    pub link: u32,
    pub info: u32,
    pub address_alignment: u64,
    pub entity_size: u64,
}

/// Defines a symbol in an ELF file.
#[derive(Clone, Debug)]
pub struct ElfSymbol {
    pub section_index: usize,
    pub symbol_index: usize,
    pub name: String,
    pub name_demangled: Option<String>,
    // st_info is a combo of a type and binding. We deconstruct.
    pub typ: u8,
    pub bind: u8,
    pub visibility: u8,
    pub section_header_index: u16,
    pub value: u64,
    pub size: u64,
    pub version_file: Option<String>,
    pub version_version: Option<String>,
}

/// Complete information about an indexed ELF file.
#[derive(Clone, Debug, Default)]
pub struct ElfBinaryInfo {
    // ELF file header fields.
    pub class: u8,
    pub data_encoding: u8,
    pub os_abi: u8,
    pub abi_version: u8,
    pub object_file_type: u16,
    pub machine: u16,
    pub entry_address: u64,
    pub elf_flags: u32,
    pub program_header_size: u16,
    pub program_header_len: u16,
    pub section_header_size: u16,
    pub section_header_len: u16,

    // Dynamic section metadata.
    /// Dynamic libraries needed by this binary.
    pub needed_libraries: Vec<String>,
    pub plt_relocs_size: Option<u64>,
    pub rela_relocs_size: Option<u64>,
    pub string_table_size: Option<u64>,
    pub init_function_address: Option<u64>,
    pub termination_function_address: Option<u64>,
    pub shared_object_name: Option<String>,
    pub rel_relocs_size: Option<u64>,
    pub flags: Option<u64>,
    pub flags1: Option<u64>,
    pub runpath: Option<String>,

    // Fields derived from sections.
    pub sections: Vec<ElfSection>,
    pub relocations_count: Option<u64>,
    pub relocations_a_count: Option<u64>,
    pub symbols: Vec<ElfSymbol>,
    pub dynamic_symbols: Vec<ElfSymbol>,

    /// Counts of machine instructions in this binary.
    pub instruction_counts: X86InstructionCounts,
}

/// Describes a binary file.
#[derive(Clone, Debug)]
pub struct BinaryFileInfo {
    /// Type of binary file.
    pub format: BinaryFormat,
    /// Flavor of binary file.
    pub kind: ObjectKind,
    /// Machine architecture.
    pub architecture: Architecture,
    /// ELF specific binary info.
    pub elf: Option<ElfBinaryInfo>,
}

/// Analyze binary info from file data.
pub fn analyze_binary_file_data(file_data: &[u8]) -> Result<Option<BinaryFileInfo>> {
    let of = match object::File::parse(file_data) {
        Ok(of) => of,
        Err(_) => return Ok(None),
    };

    if matches!(of.architecture(), Architecture::Unknown) {
        // Probably a false positive on the ELF header.
        return Ok(None);
    }

    let mut bi = BinaryFileInfo {
        format: of.format(),
        kind: of.kind(),
        architecture: of.architecture(),
        elf: None,
    };

    match FileKind::parse(file_data)? {
        FileKind::Elf32 => {
            analyze_elf::<elf::FileHeader32<Endianness>>(file_data, &mut bi)?;
        }
        FileKind::Elf64 => {
            analyze_elf::<elf::FileHeader64<Endianness>>(file_data, &mut bi)?;
        }
        _ => {}
    }

    Ok(Some(bi))
}

/// Analyze ELF data and store results in a [BinaryFileInfo].
pub fn analyze_elf<Elf: ElfFileHeader<Endian = Endianness>>(
    data: &[u8],
    bi: &mut BinaryFileInfo,
) -> Result<()> {
    let f = Elf::parse(data)?;
    let endian = f.endian()?;

    let mut ebi = ElfBinaryInfo {
        class: f.e_ident().class,
        data_encoding: f.e_ident().data,
        os_abi: f.e_ident().os_abi,
        abi_version: f.e_ident().abi_version,
        object_file_type: f.e_type(endian),
        machine: f.e_machine(endian),
        entry_address: f.e_entry(endian).into(),
        elf_flags: f.e_flags(endian),
        program_header_size: f.e_phentsize(endian),
        program_header_len: f.e_phnum(endian),
        section_header_size: f.e_shentsize(endian),
        section_header_len: f.e_shnum(endian),
        ..Default::default()
    };

    let sections = f.sections(endian, data)?;

    let versions = sections.versions(endian, data)?;

    // The object crate's symbol versioning APIs throw away the file name. So we create our
    // own mapping of symbol version index to filename.
    let symbol_version_files =
        if let Some((mut verneed, index)) = sections.gnu_verneed(endian, data)? {
            let strings = sections.strings(endian, data, index)?;
            let mut files = HashMap::new();

            while let Some((entry, mut vernaux_entries)) = verneed.next()? {
                let file = String::from_utf8_lossy(entry.file(endian, strings)?).to_string();

                while let Some(vernaux) = vernaux_entries.next()? {
                    let index = vernaux.vna_other.get(endian) & elf::VERSYM_VERSION;

                    files.insert(index, file.clone());
                }
            }

            Some(files)
        } else {
            None
        };

    for (section_index, section) in sections.iter().enumerate() {
        ebi.sections.push(ElfSection {
            index: section_index,
            name: String::from_utf8_lossy(sections.section_name(endian, section)?).to_string(),
            typ: section.sh_type(endian),
            flags: section.sh_flags(endian).into(),
            address: section.sh_addr(endian).into(),
            offset: section.sh_offset(endian).into(),
            size: section.sh_size(endian).into(),
            link: section.sh_link(endian),
            info: section.sh_info(endian),
            address_alignment: section.sh_addralign(endian).into(),
            entity_size: section.sh_entsize(endian).into(),
        });

        if let Some(symbols) =
            section.symbols(endian, data, &sections, SectionIndex(section_index))?
        {
            let strings = symbols.strings();

            for (symbol_index, symbol) in symbols.iter().enumerate() {
                let name = String::from_utf8_lossy(symbol.name(endian, strings)?);
                let demangled = symbolic_demangle::demangle(name.as_ref());

                // If symbol versions are defined and we're in the .dynsym section, there should
                // be version info for every symbol.
                let (version_file, version_version) =
                    if section.sh_type(endian) == elf::SHT_DYNSYM {
                        if let Some(versions) = &versions {
                            let version_index = versions.version_index(endian, symbol_index);

                            if let Some(version) = versions.version(version_index)? {
                                // If the symbol is undefined, we should find an entry in verneed. That
                                // means symbol_version_files should be defined and have the specified
                                // index.
                                //
                                // Otherwise, we have just a verdef and only have the version string.
                                let version = String::from_utf8_lossy(version.name()).to_string();

                                let version_file = if symbol.is_undefined(endian) {
                                    Some(symbol_version_files
                                .as_ref()
                                .ok_or_else(|| {
                                    anyhow!("symbol version filenames should be available")
                                })?
                                .get(&version_index.index())
                                .ok_or_else(|| {
                                    anyhow!("symbol version filename value should be available")
                                })?
                                .to_string())
                                } else {
                                    None
                                };

                                (version_file, Some(version))
                            } else {
                                (None, None)
                            }
                        } else {
                            (None, None)
                        }
                    } else {
                        (None, None)
                    };

                let symbol = ElfSymbol {
                    section_index,
                    symbol_index,
                    name: name.to_string(),
                    name_demangled: if name != demangled {
                        Some(demangled.to_string())
                    } else {
                        None
                    },
                    bind: symbol.st_bind(),
                    typ: symbol.st_type(),
                    visibility: symbol.st_visibility(),
                    section_header_index: symbol.st_shndx(endian),
                    value: symbol.st_value(endian).into(),
                    size: symbol.st_value(endian).into(),
                    version_file,
                    version_version,
                };

                if section.sh_type(endian) == elf::SHT_SYMTAB {
                    ebi.symbols.push(symbol);
                } else {
                    ebi.dynamic_symbols.push(symbol);
                }
            }
        }

        if let Some((rel, _)) = section.rel(endian, data)? {
            ebi.relocations_count = Some(rel.len() as _);
        }
        if let Some((rela, _)) = section.rela(endian, data)? {
            ebi.relocations_a_count = Some(rela.len() as _);
        }

        if let Some((entries, index)) = section.dynamic(endian, data)? {
            let strings = sections.strings(endian, data, index).unwrap_or_default();

            for entry in entries {
                let value_u64: u64 = entry.d_val(endian).into();

                match entry.tag32(endian) {
                    Some(elf::DT_NEEDED) => {
                        let value = entry.string(endian, strings)?;

                        ebi.needed_libraries
                            .push(String::from_utf8_lossy(value).to_string());
                    }
                    Some(elf::DT_PLTRELSZ) => {
                        ebi.plt_relocs_size = Some(value_u64);
                    }
                    Some(elf::DT_RELASZ) => {
                        ebi.rela_relocs_size = Some(value_u64);
                    }
                    Some(elf::DT_STRSZ) => {
                        ebi.string_table_size = Some(value_u64);
                    }
                    Some(elf::DT_INIT) => ebi.init_function_address = Some(value_u64),
                    Some(elf::DT_FINI) => {
                        ebi.termination_function_address = Some(value_u64);
                    }
                    Some(elf::DT_SONAME) => {
                        ebi.shared_object_name = Some(
                            String::from_utf8_lossy(entry.string(endian, strings)?).to_string(),
                        );
                    }
                    Some(elf::DT_RELSZ) => {
                        ebi.rel_relocs_size = Some(value_u64);
                    }
                    Some(elf::DT_FLAGS) => {
                        ebi.flags = Some(value_u64);
                    }
                    Some(elf::DT_RUNPATH) => {
                        ebi.runpath = Some(
                            String::from_utf8_lossy(entry.string(endian, strings)?).to_string(),
                        )
                    }
                    Some(elf::DT_FLAGS_1) => {
                        ebi.flags1 = Some(value_u64);
                    }
                    _ => {}
                }
            }
        }

        // Looks like a section containing code. Let's disassemble.
        if section.sh_type(endian) == elf::SHT_PROGBITS
            && section.sh_flags(endian).into() & u64::from(elf::SHF_EXECINSTR) != 0
        {
            // We can only disassemble x86.
            let address_size = match f.e_machine(endian) {
                elf::EM_386 => 32,
                elf::EM_X86_64 => {
                    if f.is_class_64() {
                        64
                    } else {
                        32
                    }
                }
                _ => {
                    continue;
                }
            };

            let mut decoder = iced_x86::Decoder::new(
                address_size,
                section.data(endian, data)?,
                iced_x86::DecoderOptions::AMD,
            );

            while decoder.can_decode() {
                let instruction = decoder.decode();

                /*
                // Disassemblers often skip over data definitions and NULL bytes. Should we do the
                // same?
                if matches!(instruction.code(), iced_x86::Code::INVALID) {
                    let pos = decoder.position();

                    if pos + 4 < section_data.len() {
                        println!("{}: {:x?}", pos, &section_data[pos..pos + 4]);
                    }
                }
                 */

                let count = ebi.instruction_counts.inner.entry(instruction).or_default();
                *count += 1;
            }
        }
    }

    bi.elf = Some(ebi);

    Ok(())
}
