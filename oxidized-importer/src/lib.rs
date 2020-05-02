// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use pyembed;
use python3_sys as pyffi;

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn PyInit_oxidized_importer() -> *mut pyffi::PyObject {
    pyembed::PyInit_oxidized_importer()
}
