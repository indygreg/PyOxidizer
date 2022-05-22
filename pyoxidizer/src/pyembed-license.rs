// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub fn pyembed_licenses() -> anyhow::Result<Vec<tugger_licensing::LicensedComponent>> {
    let mut res = vec![];

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("adler", "0BSD OR Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("aho-corasick", "MIT OR Unlicense")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("anyhow", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("autocfg", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("base64", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("bitflags", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("block-buffer", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("byteorder", "MIT OR Unlicense")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("bzip2", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("bzip2-sys", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("cc", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("cfg-if", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("charset", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("cmake", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("cpufeatures", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("crc32fast", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("crossbeam-utils", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("crypto-common", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("cty", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("data-encoding", "MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("digest", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("dunce", "CC0-1.0")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("either", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx(
        "encoding_rs",
        "Apache-2.0 OR BSD-3-Clause OR MIT",
    )?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("fastrand", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("flate2", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("fnv", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("fs_extra", "MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("generic-array", "MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("getrandom", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("indoc", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("instant", "BSD-3-Clause")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("itertools", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("itoa", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("jemalloc-sys", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("jobserver", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("lazy_static", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("libc", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("libmimalloc-sys", "MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("lock_api", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("mailparse", "0BSD")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("memchr", "MIT OR Unlicense")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("memmap2", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("memory-module-sys", "MPL-2.0")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("miniz_oxide", "Apache-2.0 OR MIT OR Zlib")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("num_threads", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("once_cell", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("parking_lot", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("parking_lot_core", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("pathdiff", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("pkg-config", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("ppv-lite86", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("proc-macro2", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("pyembed", "MPL-2.0 OR Python-2.0")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("pyo3", "Apache-2.0")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("pyo3-build-config", "Apache-2.0")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("pyo3-ffi", "Apache-2.0")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("pyo3-macros", "Apache-2.0")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("pyo3-macros-backend", "Apache-2.0")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx(
        "python-oxidized-importer",
        "MPL-2.0 OR Python-2.0",
    )?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("python-packaging", "MPL-2.0")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("python-packed-resources", "MPL-2.0")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("quick-error", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("quickcheck", "MIT OR Unlicense")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("quote", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("quoted_printable", "0BSD")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("rand", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("rand_chacha", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("rand_core", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("redox_syscall", "MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("regex", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("regex-syntax", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("remove_dir_all", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("rusty-fork", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("same-file", "MIT OR Unlicense")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("scopeguard", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("serde", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("serde_derive", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("sha2", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("smallvec", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("snmalloc-sys", "MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("spdx", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("spin", "MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("subtle", "BSD-3-Clause")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("syn", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx(
        "target-lexicon",
        "Apache-2.0 WITH LLVM-exception",
    )?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("tempfile", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("time", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("time-macros", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("tugger-file-manifest", "MPL-2.0")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("tugger-licensing", "MPL-2.0")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("typenum", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("unicode-ident", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("unindent", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("version_check", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("wait-timeout", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("walkdir", "MIT OR Unlicense")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx(
        "wasi",
        "Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT",
    )?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("winapi", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx(
        "winapi-i686-pc-windows-gnu",
        "Apache-2.0 OR MIT",
    )?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("winapi-util", "MIT OR Unlicense")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx(
        "winapi-x86_64-pc-windows-gnu",
        "Apache-2.0 OR MIT",
    )?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("windows-sys", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("windows_aarch64_msvc", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("windows_i686_gnu", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("windows_i686_msvc", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("windows_x86_64_gnu", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component =
        tugger_licensing::LicensedComponent::new_spdx("windows_x86_64_msvc", "Apache-2.0 OR MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    let mut component = tugger_licensing::LicensedComponent::new_spdx("zip", "MIT")?;
    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);
    res.push(component);

    Ok(res)
}
