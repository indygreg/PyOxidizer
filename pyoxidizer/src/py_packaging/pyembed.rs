// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Functionality related to the pyembed crate.
*/

use {
    anyhow::Result,
    itertools::Itertools,
    std::{fs::File, io::Write, path::Path},
};

/// Write a standalone .rs file containing a function for obtaining the default OxidizedPythonInterpreterConfig.
pub fn write_default_python_config_rs(path: &Path, python_config_rs: &str) -> Result<()> {
    let mut f = File::create(&path)?;

    // Ideally we would have a const struct, but we need to do some
    // dynamic allocations. Using a function avoids having to pull in a
    // dependency on lazy_static.
    let indented = python_config_rs
        .split('\n')
        .map(|line| "    ".to_owned() + line)
        .join("\n");

    f.write_fmt(format_args!(
        "/// Obtain the default Python configuration\n\
         ///\n\
         /// The crate is compiled with a default Python configuration embedded\n\
         /// in the crate. This function will return an instance of that\n\
         /// configuration.\n\
         pub fn default_python_config<'a>() -> pyembed::OxidizedPythonInterpreterConfig<'a> {{\n{}\n}}\n",
        indented
    ))?;

    Ok(())
}
