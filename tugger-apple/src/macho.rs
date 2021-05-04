// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::Result,
    goblin::mach::fat::{FatArch, FAT_MAGIC, SIZEOF_FAT_ARCH, SIZEOF_FAT_HEADER},
    scroll::{IOwrite, Pwrite},
    std::io::Write,
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum UniversalMachOError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("mach-o parse error: {0}")]
    Goblin(#[from] goblin::error::Error),

    #[error("scroll error: {0}")]
    Scroll(#[from] scroll::Error),
}

/// Create a universal mach-o binary from existing mach-o binaries.
///
/// The binaries will be parsed as Mach-O.
///
/// Because the size of the individual Mach-O binaries must be written into a
/// header, all content is buffered internally.
pub fn create_universal_macho<'a>(
    writer: &mut impl Write,
    binaries: impl Iterator<Item = &'a [u8]>,
) -> Result<(), UniversalMachOError> {
    // Binaries are aligned on page boundaries. x86-64 appears to use
    // 4k. aarch64 16k. It really doesn't appear to matter unless you want
    // to minimize binary size, so we always use 16k.
    const ALIGN_VALUE: u32 = 14;
    let align: u32 = 2u32.pow(ALIGN_VALUE);

    let mut records = vec![];

    let mut offset: u32 = align;

    for binary in binaries {
        let macho = goblin::mach::MachO::parse(binary, 0)?;

        // This will be 0 for the 1st binary.
        let pad_bytes = match offset % align {
            0 => 0,
            x => align - x,
        };

        offset += pad_bytes;

        let arch = FatArch {
            cputype: macho.header.cputype,
            cpusubtype: macho.header.cpusubtype,
            offset,
            size: binary.len() as u32,
            align: ALIGN_VALUE,
        };

        offset += arch.size;

        records.push((arch, pad_bytes as usize, binary));
    }

    // Fat header is the magic plus the number of records.
    writer.iowrite_with(FAT_MAGIC, scroll::BE)?;
    writer.iowrite_with(records.len() as u32, scroll::BE)?;

    for (fat_arch, _, _) in &records {
        let mut buffer = [0u8; SIZEOF_FAT_ARCH];
        buffer.pwrite_with(fat_arch, 0, scroll::BE)?;
        writer.write_all(&buffer)?;
    }

    // Pad NULL until first mach-o binary.
    let current_offset = SIZEOF_FAT_HEADER + records.len() * SIZEOF_FAT_ARCH;
    writer.write_all(&b"\0".repeat(align as usize - current_offset % align as usize))?;

    // This input would be nonsensical. Let's not even support it.
    assert!(current_offset <= align as usize, "too many mach-o entries");

    for (_, pad_bytes, macho_data) in records {
        writer.write_all(&b"\0".repeat(pad_bytes))?;
        writer.write_all(macho_data)?;
    }

    Ok(())
}
