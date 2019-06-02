// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Manage an embedded Python interpreter.

The `pyembed` crate contains functionality for managing a Python interpreter
embedded in the current binary. This crate is typically used along with
[PyOxidizer](https://github.com/indygreg/PyOxidizer) for producing
self-contained binaries containing Python.

The most important types are [`PythonConfig`](struct.PythonConfig.html) and
[`MainPythonInterpreter`](struct.MainPythonInterpreter.html). A `PythonConfig`
defines how a Python interpreter is to behave. A `MainPythonInterpreter`
creates and manages that interpreter and serves as a high-level interface for
running code in the interpreter.

`pyembed` provides significant additional functionality over what is covered
by the official [Embedding Python in Another Application](https://docs.python.org/3.7/extending/embedding.html)
docs and provided by the [CPython C API](https://docs.python.org/3.7/c-api/).
For example, `pyembed` defines a custom Python *meta path importer* that can
import Python module bytecode from memory using 0-copy. This added functionality
is the *magic sauce* that makes `pyembed`/PyOxidizer stand out from other tools
in this space.

From a very high level, this crate serves as a bridge between Rust and various
Python C APIs for interfacing with an in-process Python interpreter. This crate
*could* potentially be used as a generic interface to any linked/embedded Python
distribution. However, this crate is optimized for use with embedded Python
interpreters produced with [PyOxidizer](https://github.com/indygreg/PyOxidizer).
Use of this crate without PyOxidizer is strongly discouraged at this time.

# Dependencies

Under the hood, this crate makes direct use of the `python-sys` crate for
low-level Python FFI bindings as well as the `cpython` crate for higher-level
interfacing. Due to our special needs, **we currently require a fork of these
crates**. These forks are maintained in the
[canonicalGit repository](https://github.com/indygreg/PyOxidizer.git).
Customizations to these crates are actively upstreamed and the requirement
to use a fork should go away in time.

**It is an explicit goal of this crate to rely on as few external dependencies
as possible.** This is because we want to minimize bloat in produced binaries.
At this time, we have required direct dependencies on published versions of the
`byteorder`, `libc`, and `uuid` crates and on unpublished/forked versions
of the `python3-sys` and `cpython` crates. We also have an optional direct
dependency on the `jemalloc-sys` crate. Via the `cpython` crate, we also have an
indirect dependency on the `num-traits` crate.

This crate requires linking against a library providing CPython C symbols.
(This dependency is via the `python3-sys` crate.) On Windows, this library
must be named `pythonXY`. This library is typically generated with
PyOxidizer. As such, there is typically a build dependency on the `pyoxidizer`
crate and the crate's build script will call a function in `pyoxidizer`
to produce said library and other artifacts used for a self-contained,
embedded Python interpreter. `pyoxidizer` has 100+ crate dependencies.

# Features

The optional `jemalloc-sys` feature controls support for using
[jemalloc](http://jemalloc.net/) as Python's memory allocator. This feature is
enabled by default. Use of Jemalloc from Python is a run-time configuration
option controlled by the [`PythonConfig`](struct.PythonConfig.html) type
and having `jemalloc` compiled into the binary does not mean it is being used!

# Technical Implementation Details

When trying to understand the code, a good place to start is
[`MainPythonInterpreter.new()`](struct.MainPythonInterpreter.html#method.new),
as this will initialize the CPython runtime and Python initialization is
where most of the magic occurs.

A lot of initialization code revolves around mapping
[`PythonConfig`](struct.PythonConfig.html)` members to C API calls e.g. the
`program_name` field translates to a call to `Py_SetProgramName()`. This
functionality is rather straightforward. There't nothing really novel
or complicated here. So we won't cover it.

## Python Memory Allocators

There exist several
[CPython APIs for memory management](https://docs.python.org/3/c-api/memory.html).
CPython defines multiple memory allocator *domains* and it is possible to
use a custom memory allocator for each using the `PyMem_SetAllocator()` API.

We support having the *raw* memory allocator use either `jemalloc` or
Rust's global allocator.

The `pyalloc` module defines types that serve as interfaces between the
`jemalloc` library and Rust's allocator. The reason we call into
`jemalloc-sys` directly instead of going through Rust's allocator is overhead:
why involve an extra layer of abstraction when it isn't needed. To register
a custom allocator, we simply instantiate an instance of the custom allocator
type and tell Python about it via ``PyMem_SetAllocator()``.

## Module Importing

The module importing mechanisms provided by this crate are one of the
most complicated parts of the crate. This section aims to explain how it
works. But before we go into the technical details, we need an understanding
of how Python module importing works.

### High Level Python Importing Overview

A *meta path importer* is a Python object implementing
the [`importlib.abc.MetaPathFinder`](https://docs.python.org/3.7/library/importlib.html#importlib.abc.MetaPathFinder)
interface and is registered on [`sys.meta_path`](https://docs.python.org/3.7/library/sys.html#sys.meta_path).
Essentially, when the `__import__` function / `import` statement is used,
Python's importing internals traverse entities in `sys.meta_path` and
ask each member to load a module. The first *meta path importer* that knows
about the module is used.

By default, Python configures 3 *meta path importers*: an importer for
built-in extension modules (`BuiltinImporter`), frozen modules (`FrozenImporter`),
and filesystem-based modules (`PathFinder`). You can see these on a fresh
Python interpreter:

   $ python3.7 -c 'import sys; print(sys.meta_path)`
   [<class '_frozen_importlib.BuiltinImporter'>, <class '_frozen_importlib.FrozenImporter'>, <class '_frozen_importlib_external.PathFinder'>]

These types are all implemented in Python code in the Python standard
library, specifically in the `importlib._bootstrap` and
`importlib._bootstrap_external` modules.

Built-in extension modules are compiled into the Python library. These are often
extension modules required by core Python (such as the `_codecs`, `_io`, and
`_signal` modules). But it is possible for other extensions - such as those
provided by Python's standard library or 3rd party packages - to exist as
built-in extension modules as well.

For importing built-in extension modules, there's a global `PyImport_Inittab`
array containing members defining the extension/module name and a pointer to
its C initialization function. There are undocumented functions exported to
Python (such as `_imp.exec_builtin()` that allow Python code to call into C code
which knows how to e.g. instantiate these extension modules. The
`BuiltinImporter` calls into these C-backed functions to service imports of
built-in extension modules.

Frozen modules are Python modules that have their bytecode backed by memory.
There is a global `PyImport_FrozenModules` array that - like `PyImport_Inittab` -
defines module names and a pointer to bytecode data. The `FrozenImporter`
calls into undocumented C functions exported to Python to try to service
import requests for frozen modules.

Path-based module loading via the `PathFinder` meta path importer is what
most people are likely familiar with. It uses `sys.path` and a handful of
other settings to traverse filesystem paths, looking for modules in those
locations. e.g. if `sys.path` contains
`['', '/usr/lib/python3.7', '/usr/lib/python3.7/lib-dynload', '/usr/lib/python3/dist-packages']`,
`PathFinder` will look for `.py`, `.pyc`, and compiled extension modules
(`.so`, `.dll`, etc) in each of those paths to service an import request.
Path-based module loading is a complicated beast, as it deals with all
kinds of complexity like caching bytecode `.pyc` files, differentiating
between Python modules and extension modules, namespace packages, finding
search locations in registry entries, etc. Altogether, there are 1500+ lines
constituting path-based importing logic in `importlib._bootstrap_external`!

### Default Initialization of Python Importing Mechanism

CPython's internals go through a convoluted series of steps to initialize
the importing mechanism. This is because there's a bit of chicken-and-egg
scenario going on. The *meta path importers* are implemented as Python
modules using Python source code (`importlib._bootstrap` and
`importlib._bootstrap_external`). But in order to execute Python code you
need an initialized Python interpreter. And in order to execute a Python
module you need to import it. And how do you do any of this if the importing
functionality is implemented as Python source code and as a module?!

A few tricks are employed.

At Python build time, the source code for `importlib._bootstrap` and
`importlib._bootstrap_external` are compiled into bytecode. This bytecode is
made available to the global `PyImport_FrozenModules` array as the
`_frozen_importlib` and `_frozen_importlib_external` module names,
respectively. This means the bytecode is available for Python to load
from memory and the original `.py` files are not needed.

During interpreter initialization, Python initializes some special
built-in extension modules using its internal import mechanism APIs. These
bypass the Python-based APIs like `__import__`. This limited set of
modules includes `_imp` and `sys`, which are both completely implemented in C.

During initialization, the interpreter also knows to explicitly look for
and load the `_frozen_importlib` module from its frozen bytecode. It creates
a new module object by hand without going through the normal import mechanism.
It then calls the `_install()` function in the loaded module. This function
executes Python code on the partially bootstrapped Python interpreter which
culminates with `BuiltinImporter` and `FrozenImporter` being registered on
`sys.meta_path`. At this point, the interpreter can import compiled
built-in extension modules and frozen modules. Subsequent interpreter
initialization henceforth uses the initialized importing mechanism to
import modules via normal import means.

Later during interpreter initialization, the `_frozen_importlib_external`
frozen module is loaded from bytecode and its `_install()` is also called.
This self-installation adds `PathFinder` to `sys.meta_path`. At this point,
modules can be imported from the filesystem. This includes `.py` based modules
from the Python standard library as well as any 3rd party modules.

Interpreter initialization continues on to do other things, such as initialize
signal handlers, initialize the filesystem encoding, set up the `sys.std*`
streams, etc. This involves importing various `.py` backed modules (from the
filesystem). Eventually interpreter initialization is complete and the
interpreter is ready to execute the user's Python code!

### Our Importing Mechanism

We have made significant modifications to how the Python importing
mechanism is initialized and configured. (Note: we do not require these
modifications. It is possible to initialize a Python interpreter with
*default* behavior, without support for in-memory module importing.)

The `importer` module of this crate defines a Python extension module.
To the Python interpreter, an extension module is a C function that calls
into the CPython C APIs and returns a `PyObject*` representing the
constructed Python module object. This extension module behaves like any
other extension module you've seen. The main differences are it is implemented
in Rust (instead of C) and it is compiled into the binary containing Python,
as opposed to being a standalone shared library that is loaded into the Python
process.

This extension module provides the `_pyoxidizer_importer` Python module,
which provides a global `_setup()` function to be called from Python.

The [`PythonConfig`](struct.PythonConfig.html) instance used to construct
the Python interpreter contains a `&[u8]` referencing bytecode to be loaded
as the `_frozen_importlib` and `_frozen_importlib_external` modules. The
bytecode for `_frozen_importlib_external` is compiled from a **modified**
version of the original `importlib._bootstrap_external` module provided by
the Python interpreter. This custom module version defines a *new*
`_install()` function which effectively runs
`import _pyoxidizer_importer; _pyoxidizer_importer._setup(...)`.

When we initialize the Python interpreter, the `_pyoxidizer_importer`
extension module is appended to the global `PyImport_Inittab` array,
allowing it to be recognized as a *built-in* extension module and
imported as such. In addition, the global `PyImport_FrozenModules` array
is modified so the `_frozen_importlib` and `_frozen_importlib_external`
modules point at our modified bytecode provided by `PythonConfig`.

When `Py_Initialize()` is called, the initialization proceeds as before.
`_frozen_importlib._install()` is called to register `BuiltinImporter`
and `FrozenImporter` on `sys.meta_path`. This is no different from
vanilla Python. When `_frozen_importlib_external._install()` is called,
our custom version/bytecode runs. It performs an
`import _pyoxidizer_importer`, which is serviced by `BuiltinImporter`.
Our Rust-implemented module initialization function runs and creates
a module object. We then call `_setup()` on this module to complete
the logical initialization.

The role of the `_setup()` function in our extension module is to add
a new *meta path importer* to `sys.meta_path`. The chief goal of our
importer is to support importing Python modules from memory using 0-copy.

Our extension module grabs a handle on the `&[u8]` containing modules
data embedded into the binary. (See below for the format of this blob.)
The in-memory data structure is parsed into a Rust collection type
(basically a `HashMap<&str, (&[u8], &[u8])>`) mapping Python module names
to their source and bytecode data.

The extension module defines a `PyOxidizerFinder` Python type that
implements the requisite `importlib.abc.*` interfaces for providing a
*meta path importer*. An instance of this type is constructed from the
parsed data structure containing known Python modules. That instance is
registered as the first entry on `sys.meta_path`.

When our module's `_setup()` completes, control is returned to
`_frozen_importlib_external._install()`, which finishes and returns
control to whatever called it.

As `Py_Initialize()` and later user code runs its course, requests are
made to import non-built-in, non-frozen modules. (These requests are
usually serviced by `PathFinder` via the filesystem.) The standard
`sys.meta_path` traversal is performed. The Rust-implemented
`PyOxidizerFinder` converts the requested Python module name to a Rust
`&str` and does a lookup in a `HashMap<&str, ...>` to see if it knows
about the module. Assuming the module is found, a `&[u8]` handle on
that module's source or bytecode is obtained. That pointer is used to
construct a Python `memoryview` object, which allows Python to access
the raw bytes without a memory copy. Depending on the type, the source
code is decoded to a Python `str` or the bytecode is sent to
`marshal.loads()`, converted into a Python `code` object, which is then
executed via the equivalent of `exec(code, module.__dict__)` to populate
an empty Python module object.

In addition, `PyOxidizerFinder` indexes the built-in extension modules
and frozen modules. It removes `BuiltinImporter` and `FrozenImporter`
from `sys.meta_path`. When `PyOxidizerFinder` sees a request for a
built-in or frozen module, it dispatches to `BuiltinImporter` or
`FrozenImporter` to complete the request. The reason we do this is
performance. Imports have to traverse `sys.meta_path` entries. So the
fewer entries there are, the more overhead there is. Compounding the
problem is that `BuiltinImporter` and `FrozenImporter` do a `strcmp()`
against the global module arrays when trying to service an import.
`PyOxidizerFinder` already has an index of module name to data. So it
was not that much effort to also index built-in and frozen modules
so there's a fixed, low cost for finding modules.

It's worth explicitly noting that it is important for our custom code
to run *before* `_frozen_importlib_external._install()` completes. This
is because Python interpreter initialization relies on the fact that
`.py` implemented standard library modules are available for import
during initialization. For example, initializing the filesystem encoding
needs to import the `encodings` module, which is provided by a `.py` file
on the filesystem in standard installations.

**It is impossible to provide in-memory importing of the entirety of the
Python standard library without injecting custom code while
`Py_Initialize()` is running.** This is because `Py_Initialize()` imports
modules from the filesystem. And, a subset of these standard library
modules don't work as *frozen* modules. (The `FrozenImporter` doesn't
set all required module attributes, leading to failures relying on
missing attributes.)

# Packed Modules Data

The custom meta path importer provided by this crate supports importing
Python modules data (source and bytecode) from memory using 0-copy. The
[`PythonConfig`](struct.PythonConfig.html) simply references a `&[u8]`
(a generic slice over bytes data) providing modules data in a packed format.

The format of this packed data is as follows.

The first 4 bytes are a little endian u32 containing the total number of
modules in this data. Let's call this value `total`.

Following is an array of length `total` with each array element being
a 3-tuples of packed (no interior or exterior padding) composed of 3
little endian u32 values. These values correspond to the module name
length (`name_length`), module source data length (`source_length`),
and module bytecode data length (`bytecode_length`), respectively.

Following the lengths array is a vector of the module name strings.
This vector has `total` elements. Each element is a non-NULL terminated
`str` of the `name_length` specified by the corresponding entry in the
lengths array. There is no padding between values. Values MUST be valid
UTF-8 (they should be ASCII).

Following the names array is a vector of the module sources. This
vector has `total` elements and behaves just like the names vector,
except the `source_length` field from the lengths array is used.

Following the sources array is a vector of the module bytecodes. This
behaves identically to the sources vector except the `bytecode_length`
field from the lengths array is used.

Example (without literal integer encoding and spaces for legibility):

```text
2                     # Total number of elements

[                     # Array defining 2 modules. 24 bytes total because 2 12
                      # byte members.
   (3, 0, 1024),      # 1st module has name of length 3, no source data,
                      # 1024 bytes of bytecode

   (4, 192, 4213),    # 2nd module has name length 4, 192 bytes of source
                      # data, 4213 bytes of bytecode
]

foomain               # "foo" + "main" module names, of lengths 3 and 4,
                      # respectively.

# This is main.py.\n  # 192 bytes of source code for the "main" module.

<binary data>         # 1024 + 4213 bytes of Python bytecode data.
```

The design of the format was influenced by a handful of considerations.

Performance is a significant consideration. We want everything to be as
fast as possible.

The *index* data is located at the beginning of the structure so a reader
only has to read a contiguous slice of data to fully parse the index. This
is in opposition to jumping around the entire backing slice to extract useful
data.

x86 is little endian, so little endian integers are used so integer translation
doesn't need to be performed.

It is assumed readers will want to construct an index of known modules. All
module names are tightly packed together so a reader doesn't need to read
small pieces of data from all over the backing slice. Similarly, it is assumed
that similar data types will be accessed together. This is why source and
bytecode data are packed with each other instead of packed per-module.

Everything is designed to facilitate 0-copy. So Rust need only construct a
`&[u8]` into the backing slice to reference raw data.

Since Rust is the intended target, string data (module names) are not NULL
terminated / C strings because Rust's `str` are not NULL terminated.

It is assumed that the module data is baked into the binary and is therefore
trusted/well-defined. There's no *version header* or similar because data
type mismatch should not occur. A version header should be added in the
future because that's good data format design, regardless of assumptions.

There is no checksumming of the data because we don't want to incur
I/O overhead to read the entire blob. It could be added as an optional
feature.

Currently, the format requires the parser to perform offset math to
compute slices of data. A potential area for improvement is for the
index to contain start offsets and lengths so the parser can be more
*dumb*. It is unlikely this has performance implications because integer
math is fast and any time spent here is likely dwarfed by Python interpreter
startup overhead.

Another potential area for optimization is module name encoding. Module
names could definitely compress well. But use of compression will undermine
0-copy properties. Similar compression opportunities exist for source and
bytecode data with similar caveats.

*/

mod config;
mod data;
mod importer;
mod pyalloc;
mod pyinterp;
mod pystr;

#[allow(unused_imports)]
pub use config::PythonConfig;

#[allow(unused_imports)]
pub use data::default_python_config;

#[allow(unused_imports)]
pub use pyinterp::MainPythonInterpreter;
