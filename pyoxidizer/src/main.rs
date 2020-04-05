// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!

`PyOxidizer` is a tool and library for producing binaries that embed
Python.

The over-arching goal of `PyOxidizer` is to make complex Python
packaging and distribution problems simple so application maintainers
can focus on building quality applications instead of toiling with
build systems and packaging tools.

`PyOxidizer` is capable of producing a self-contained executable containing
a fully-featured Python interpreter and all Python modules required to run
a Python application. On Linux, it is possible to create a fully static
executable that doesn't even support dynamic loading and can run on nearly
every Linux machine.

The *Oxidizer* part of the name comes from Rust: binaries built with
`PyOxidizer` are compiled from Rust and Rust code is responsible for
managing the embedded Python interpreter and all its operations. But the
existence of Rust should be invisible to many users, much like the fact
that CPython (the official Python distribution available from www.python.org)
is implemented in C. Rust is simply a tool to achieve an end goal, albeit
a rather effective and powerful tool.
*/

mod analyze;
#[allow(unused)]
pub mod app_packaging;
mod cli;
//mod distribution;
mod environment;
mod licensing;
mod logging;
mod project_building;
mod project_layout;
mod projectmgmt;
mod py_packaging;
mod python_distributions;
pub mod starlark;
#[cfg(test)]
mod testutil;

fn main() {
    std::process::exit(match cli::run_cli() {
        Ok(_) => 0,
        Err(e) => {
            println!("error: {}", e);
            1
        }
    });
}
