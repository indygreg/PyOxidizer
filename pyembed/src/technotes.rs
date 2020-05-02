// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Technical Implementation Notes

When trying to understand the code, a good place to start is
`MainPythonInterpreter.new()`, as this will initialize the CPython runtime and
Python initialization is where most of the magic occurs.

A lot of initialization code revolves around mapping `PythonConfig` members to
C API calls. This functionality is rather straightforward. There's
nothing really novel or complicated here. So we won't cover it.

# Python Memory Allocators

There exist several
[CPython APIs for memory management](https://docs.python.org/3/c-api/memory.html).
CPython defines multiple memory allocator *domains* and it is possible to
use a custom memory allocator for each using the `PyMem_SetAllocator()` API.

We support having the *raw* memory allocator use either `jemalloc`, Rust's
global allocator, or the system allocator.

The `pyalloc` module defines types that serve as interfaces between the
`jemalloc` library and Rust's allocator. The reason we call into
`jemalloc-sys` directly instead of going through Rust's allocator is overhead:
why involve an extra layer of abstraction when it isn't needed. To register
a custom allocator, we simply instantiate an instance of the custom allocator
type and tell Python about it via `PyMem_SetAllocator()`.

# Module Importing

The module importing mechanisms provided by this crate are one of the
most complicated parts of the crate. This section aims to explain how it
works. But before we go into the technical details, we need an understanding
of how Python module importing works.

## High Level Python Importing Overview

A *meta path importer* is a Python object implementing
the [importlib.abc.MetaPathFinder](https://docs.python.org/3.7/library/importlib.html#importlib.abc.MetaPathFinder)
interface and is registered on [sys.meta_path](https://docs.python.org/3.7/library/sys.html#sys.meta_path).
Essentially, when the `__import__` function / `import` statement is called,
Python's importing internals traverse entities in `sys.meta_path` and
ask each *finder* to load a module. The first *meta path importer* that knows
about the module is used.

By default, Python configures 3 *meta path importers*: an importer for
built-in extension modules (`BuiltinImporter`), frozen modules
(`FrozenImporter`), and filesystem-based modules (`PathFinder`). You can
see these on a fresh Python interpreter:

```text
   $ python3.7 -c 'import sys; print(sys.meta_path)`
   [<class '_frozen_importlib.BuiltinImporter'>, <class '_frozen_importlib.FrozenImporter'>, <class '_frozen_importlib_external.PathFinder'>]
```

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
There is a global `PyImport_FrozenModules` array that - like
`PyImport_Inittab` - defines module names and a pointer to bytecode data. The
`FrozenImporter` calls into undocumented C functions exported to Python to try
to service import requests for frozen modules.

Path-based module loading via the `PathFinder` meta path importer is what
most people are likely familiar with. It uses `sys.path` and a handful of
other settings to traverse filesystem paths, looking for modules in those
locations. e.g. if `sys.path` contains
`['', '/usr/lib/python3.7', '/usr/lib/python3.7/lib-dynload', '/usr/lib/python3/dist-packages']`,
`PathFinder` will look for `.py`, `.pyc`, and compiled extension modules
(`.so`, `.pyd`, etc) in each of those paths to service an import request.
Path-based module loading is a complicated beast, as it deals with all
kinds of complexity like caching bytecode `.pyc` files, differentiating
between Python modules and extension modules, namespace packages, finding
search locations in registry entries, etc. Altogether, there are 1500+ lines
constituting path-based importing logic in `importlib._bootstrap_external`!

## Default Initialization of Python Importing Mechanism

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
modules includes `_imp` and `sys`, which are both completely implemented in
C.

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

## Our Importing Mechanism

We have made significant modifications to how the Python importing
mechanism is initialized and configured. (Note: we do not require these
modifications. It is possible to initialize a Python interpreter with
*default* behavior, without support for in-memory module importing.)

The `importer` Rust module of this crate defines a Python extension module.
To the Python interpreter, an extension module is a C function that calls
into the CPython C APIs and returns a `PyObject*` representing the
constructed Python module object. This extension module behaves like any
other extension module you've seen. The main differences are it is implemented
in Rust (instead of C) and it is compiled into the binary containing Python,
as opposed to being a standalone shared library that is loaded into the Python
process.

This extension module provides the `oxidized_importer` Python module,
which defines a meta path importer.

When we initialize the Python interpreter, the `oxidized_importer`
extension module is appended to the global `PyImport_Inittab` array,
allowing it to be recognized as a *built-in* extension module and
imported as such.

We use the PEP-587
[Python Initialization Configuration](https://docs.python.org/3/c-api/init_config.html)
API to have granular control over Python initialization. Our most
notable departure is we force a multi-phase initialization so
initialization pauses between _core_ and _main_ initialization.

When _core_ is initialized, `_frozen_importlib._install()` is called to
register `BuiltinImporter` and `FrozenImporter` on `sys.meta_path`.
At our break point after _core_ initialization, we import our
`oxidized_importer` module using the Python C APIs. This import
is serviced by `BuiltinImporter`. Our Rust-implemented module initialization
function runs and creates a module object. We then call another Rust
function to complete the module initialization given the current
configuration. This will create a new *meta path importer* and register
it on `sys.meta_path`. The chief goal of our importer is to support
importing Python resources using an efficient binary data structure.

Our extension module grabs a handle on the `&[u8]` containing modules
data embedded into the binary. (See
[../specifications/index.html](Specifications) for the format of this blob.)
The in-memory data structure is parsed into a Rust collection type
(basically a `HashMap<&str, (&[u8], &[u8])>`) mapping Python module names
to their source and bytecode data.

The extension module defines an `OxidizedFinder` Python type that
implements the requisite `importlib.abc.*` interfaces for providing a
*meta path importer*. An instance of this type is constructed from the
parsed data structure containing known Python modules. That instance is
registered as the first entry on `sys.meta_path`.

When our module's `_setup()` completes, we trigger the *main*
initialization. This will *always* register the traditional filesystem
importer (`PathFinder`) on `sys.meta_path`. But, since our finder is
registered first, it should always be used.

As part of _main_ interpreter initialization, Python attempts various
imports of `.py` based modules. The standard `sys.meta_path` traversal
is performed. The Rust-implemented `OxidizedFinder` converts the
requested Python module name to a Rust `&str` and does a lookup in a
`HashMap<&str, ...>` to see if it knows about the module. Assuming the
module is found, a `&[u8]` handle on that module's source or bytecode is
obtained. That pointer is used to construct a Python `memoryview` object,
which allows Python to access the raw bytes without a memory copy.
Depending on the type, the source code is decoded to a Python `str` or
the bytecode is sent to `marshal.loads()`, converted into a Python `code`
object, which is then executed via the equivalent of
`exec(code, module.__dict__)` to populate an empty Python module object.

In addition, `OxidizedFinder` indexes the built-in extension modules
and frozen modules. It removes `BuiltinImporter` and `FrozenImporter`
from `sys.meta_path`. When `OxidizedFinder` sees a request for a
built-in or frozen module, it dispatches to `BuiltinImporter` or
`FrozenImporter` to complete the request. The reason we do this is
performance. Imports have to traverse `sys.meta_path` entries until a
registered finder says it can service the request. So the more entries
there are, the more overhead there is. Compounding the problem is that
`BuiltinImporter` and `FrozenImporter` do a `strcmp()`
against the global module arrays when trying to service an import.
`OxidizedFinder` already has an index of module name to data. So it
was not that much effort to also index built-in and frozen modules
so there's a fixed, low cost for finding modules (a Rust `HashMap` key
lookup).

It's worth explicitly noting that it is important for our custom importer
to run *before* the _main_ initialization phase completes. This
is because Python interpreter initialization relies on the fact that
`.py` implemented standard library modules are available for import
during initialization. For example, initializing the filesystem encoding
needs to import the `encodings` module, which is provided by a `.py` file
on the filesystem in standard installations.

After the _main_ initialization phase completes, we remove `PathFinder`
from `sys.meta_path` if the configuration says to disable filesystem
based imports. The overhead of registering then unregistering it should
be trivial and no I/O should have been performed.

*/
