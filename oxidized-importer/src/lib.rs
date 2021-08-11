// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// clippy doesn't know about what features to pass to dependent crates.
// So pyembed isn't exporting the correct symbols and clippy will barf
// due to unbound symbols. So we suppress clippy as a workaround.

#[cfg(not(feature = "cargo-clippy"))]
use pyo3::ffi as pyffi;

#[cfg(not(feature = "cargo-clippy"))]
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn PyInit_oxidized_importer() -> *mut pyffi::PyObject {
    pyembed::PyInit_oxidized_importer()
}
