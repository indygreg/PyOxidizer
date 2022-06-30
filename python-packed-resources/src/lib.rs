// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

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
[PyOxidizer](https://gregoryszorc.com/docs/pyoxidizer/stable/pyoxidizer.html). But
it can be used outside the PyOxidizer project. See the aforementioned docs
for the canonical specification of this format.
*/

mod parser;
mod resource;
mod serialization;
mod writer;

pub use crate::{
    parser::{load_resources, ResourceParserIterator},
    resource::Resource,
    serialization::HEADER_V3,
    writer::write_packed_resources_v3,
};
