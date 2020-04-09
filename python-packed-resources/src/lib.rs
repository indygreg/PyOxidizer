// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Python Packed Resources

This crate defines and implements a data format for storing resources useful
to the execution of a Python interpreter. We call this data format *Python
packed resources*.

The idea is that a producer collects Python resources required by a Python
interpreter - Python module source and bytecode, non-module resource files,
extension modules, shared libraries, etc - attaches metadata to those
resources (e.g. whether a Python module is also a package), and then
serializes all of this out to a binary data structure.

Later, this data structure is parsed back into composite parts. e.g.
to a mapping of Python module names and their respective data. This
data structure is then consulted by a Python interpreter to e.g. power
the module `import` mechanism.

This crate is developed primarily for
[PyOxidizer](https://pyoxidizer.readthedocs.io/en/stable/). But it can
be used outside the PyOxidizer project.
*/

pub mod data;
#[allow(unused)]
pub mod parser;
pub mod specifications;
pub mod writer;
