// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for analyzing the content of platform binaries. */

mod audit;
pub use audit::{analyze_data, analyze_elf_libraries, analyze_file};
mod elf;
pub use elf::find_undefined_elf_symbols;
mod linux_distro_versions;
pub use linux_distro_versions::{
    find_minimum_distro_version, GCC_VERSIONS_BY_DISTRO, GLIBC_VERSIONS_BY_DISTRO,
};
mod pe;
pub use pe::{find_pe_dependencies, find_pe_dependencies_path};

/// Shared libraries defined as part of the Linux Shared Base specification.
pub const LSB_SHARED_LIBRARIES: &[&str] = &[
    "ld-linux-x86-64.so.2",
    "libc.so.6",
    "libdl.so.2",
    "libgcc_s.so.1",
    "libm.so.6",
    "libpthread.so.0",
    "librt.so.1",
    "libutil.so.1",
];

#[derive(Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct UndefinedSymbol {
    pub symbol: String,
    pub filename: Option<String>,
    pub version: Option<String>,
}
