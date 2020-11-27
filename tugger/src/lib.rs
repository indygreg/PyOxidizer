// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
The `tugger` crate contains functionality for packaging and distributing
software applications.

The core of `tugger` consists of a set of types for defining packaging
actions and functions to operate on them. There is a frontend component
which defines a [Starlark](https://github.com/bazelbuild/starlark)
dialect for allowing these types to be constructed from user-provided
configuration files.

Tugger is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer) project. While
developed in the same repository, Tugger is a generic, standalone
Rust crate and utility. It just happens to be developed alongside
PyOxidizer.
*/

pub mod starlark;
pub mod tarball;
