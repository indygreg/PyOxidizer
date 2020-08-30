// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// SPDX licenses in Python distributions that are not GPL.
///
/// We store an allow list of licenses rather than trying to deny GPL licenses
/// because if we miss a new GPL license, we accidentally let in GPL.
pub const NON_GPL_LICENSES: &[&str] = &[
    "BSD-3-Clause",
    "bzip2-1.0.6",
    "MIT",
    "OpenSSL",
    "Sleepycat",
    "X11",
    "Zlib",
];
