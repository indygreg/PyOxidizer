// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub fn pyembed_licenses() -> anyhow::Result<Vec<python_packaging::licensing::LicensedComponent>> {
    let mut res = vec![];

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("adler".to_string()),
        "0BSD OR MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("aho-corasick".to_string()),
        "Unlicense OR MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("anyhow".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("autocfg".to_string()),
        "Apache-2.0 OR MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("base64".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("bitflags".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("block-buffer".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("byteorder".to_string()),
        "Unlicense OR MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("bzip2".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("bzip2-sys".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("cc".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("cfg-if".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("charset".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("cmake".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("cpufeatures".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("crc32fast".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("crossbeam-utils".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("crypto-common".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("cty".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("data-encoding".to_string()),
        "MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("digest".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("dunce".to_string()),
        "CC0-1.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("either".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("encoding_rs".to_string()),
        "(Apache-2.0 OR MIT) AND BSD-3-Clause",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("flate2".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("fs_extra".to_string()),
        "MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("generic-array".to_string()),
        "MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("getrandom".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("indoc".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("itertools".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("itoa".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("jemalloc-sys".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("jobserver".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("lazy_static".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("libc".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("libmimalloc-sys".to_string()),
        "MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("lock_api".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("mailparse".to_string()),
        "0BSD",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("memchr".to_string()),
        "Unlicense OR MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("memmap2".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("memory-module-sys".to_string()),
        "MPL-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("miniz_oxide".to_string()),
        "MIT OR Zlib OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("num_threads".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("once_cell".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("parking_lot".to_string()),
        "Apache-2.0 OR MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("parking_lot_core".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("pkg-config".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("ppv-lite86".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("proc-macro2".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("pyembed".to_string()),
        "Python-2.0 OR MPL-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("pyo3".to_string()),
        "Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("pyo3-build-config".to_string()),
        "Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("pyo3-ffi".to_string()),
        "Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("pyo3-macros".to_string()),
        "Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("pyo3-macros-backend".to_string()),
        "Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate(
            "python-oxidized-importer".to_string(),
        ),
        "Python-2.0 OR MPL-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("python-packaging".to_string()),
        "MPL-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate(
            "python-packed-resources".to_string(),
        ),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("quickcheck".to_string()),
        "Unlicense OR MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("quote".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("quoted_printable".to_string()),
        "0BSD",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("rand".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("rand_chacha".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("rand_core".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("redox_syscall".to_string()),
        "MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("regex".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("regex-syntax".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("same-file".to_string()),
        "Unlicense OR MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("scopeguard".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("serde".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("serde_derive".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("sha2".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("smallvec".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("snmalloc-sys".to_string()),
        "MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("spdx".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("spin".to_string()),
        "MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("subtle".to_string()),
        "BSD-3-Clause",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("syn".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("target-lexicon".to_string()),
        "Apache-2.0 WITH LLVM-exception",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("time".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("time-macros".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("tugger-file-manifest".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("typenum".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("unicode-ident".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("unindent".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("version_check".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("walkdir".to_string()),
        "Unlicense OR MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("wasi".to_string()),
        "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("winapi".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate(
            "winapi-i686-pc-windows-gnu".to_string(),
        ),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("winapi-util".to_string()),
        "Unlicense OR MIT",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate(
            "winapi-x86_64-pc-windows-gnu".to_string(),
        ),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("windows-sys".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("windows_aarch64_msvc".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("windows_i686_gnu".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("windows_i686_msvc".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("windows_x86_64_gnu".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("windows_x86_64_msvc".to_string()),
        "MIT OR Apache-2.0",
    )?);

    res.push(python_packaging::licensing::LicensedComponent::new_spdx(
        python_packaging::licensing::ComponentFlavor::RustCrate("zip".to_string()),
        "MIT",
    )?);

    Ok(res)
}
