.. py:currentmodule:: oxidized_importer

.. _python_packed_resources:

=======================
Python Packed Resources
=======================

This project has defined a custom data format for storing resources
useful to the execution of a Python interpreter. We call this data
format *Python packed resources*.

The way it works is that some producer collects resources required by
a Python interpreter. These resources include Python module source
and bytecode, non-module resource/data files, extension modules, and
shared libraries. Metadata about these resources and sometimes the
raw resource data itself is serialized to a binary data structure.

At Python interpreter run time, an instance of the :py:class:`OxidizedFinder`
*meta path finder* parses this data structure and uses it to power Python
module importing.

This functionality is similar to using a ``.zip`` file for holding
Python modules. However, the *Python packed resources* data structure
is far more advanced.

Implementation
==============

The canonical implementation of the writer and parser of this data
structure lives in the ``python-packed-resources`` Rust crate. The
canonical home of this crate is
https://github.com/indygreg/PyOxidizer/tree/main/python-packed-resources.

This crate is published to crates.io at
https://crates.io/crates/python-packed-resources.

The ``oxidized_importer`` Rust crate / Python extension defines the
:py:class:`OxidizedFinder` Python class for using this data structure
to power importing. That extension also exposes APIs to interact with
instances of the data structure.

Concepts
========

The data structure is logically an iterable of *resources*.

A *resource* is a sparse collection of *attributes* or *fields*.

Each *attribute* describes behavior of the *resource* or defines data for that
resource. For example, there are *attributes* that denote the type of
a resource. A *Python module* *resource* might have an attribute holding
its Python sourcecode or bytecode.

In Rust speak, a *resource* is a ``struct`` and *attributes* are fields
in that ``struct``. Many fields are ``Option<T>`` because they are
optional and not always defined.

Serialization Format
====================

High-Level Overview
-------------------

The serialization format consists of:

* A *global header* containing identifying magic and describing the overall
  payload.
* An index describing data for each distinct *attribute* type. This is
  called the *blob index*.
* An index describing each resource and its attributes. This is called the
  *resources index*.
* A series of sections holding data for each distinct *attribute* type. We
  call these *blob sections*.

All integers are little-endian.

Global Header
-------------

The first 8 bytes of the data structure are a magic header identifying
the content as our data structure and the version of it. The first
7 bytes are ``pyembed`` and the following 1 byte denotes a version.
Semantics of each version are denoted in sections below.

The first 13 bytes after the magic header describe the *blob* and
*resource* indices as follows:

* A ``u8`` denoting the number of blob sections, ``blob_sections_count``.
* A ``u32`` denoting the length of the blob index, ``blob_index_length``.
* A ``u32`` denoting the total number of resources in this data,
  ``resources_count``.
* A ``u32`` denoting the length of the resources index,
  ``resources_index_length``.

Blob Index
----------

Following the *global header* is the *blob index*, which describes the
*blob sections* present later in the data structure.

Each entry in the *blob index* logically consists of a set of fields defining
metadata about each *blob section*. This is encoded by a *start of entry*
``u8`` marker followed by N ``u8`` field type values and their corresponding
metadata, followed by an *end of entry* ``u8`` marker.

The *blob index* is terminated by an *end of index* ``u8`` marker.

The total number of bytes in the *blob index* including the *end of index*
marker should be ``blob_index_length``.

The *blob index* allows attributing a sparse set of metadata with every blob
section entry. The type of metadata being conveyed is defined by a ``u8``.
Some field types have additional metadata following that field.

The various field types and their semantics follow.

``0x00``
   End of index. This field indicates that there are no more blob
   index entries and we've reached the end of the *blob index*.

``0x01``
   Start of blob section entry. Encountering this value signals the
   beginning of a new blob section. From a specification standpoint, this isn't
   strictly required. But it helps ensure parser state.

``0xff``
   End of blob section entry. Encountering this value signals the end
   of the current blob section definition. The next encountered ``u8`` in the
   index should be ``0x01`` to denote a new entry or ``0x00`` to denote end of
   index.

``0x02``
   Resource field type. This field defines which resource field this
   blob section is holding data for. A ``u8`` following this one will contain
   the resource field type value (see section below).

``0x03``
   Raw payload length. This field defines the raw length in bytes of
   the blob section in the payload. The ``u64`` containing that length will
   immediately follow this ``u8``.

``0x04``
   Interior padding mechanism. This field defines interior padding
   between elements in the blob section. Following this ``u8`` is another ``u8``
   denoting the padding mechanism.

   ``0x01`` indicates no padding.
   ``0x02`` indicates NULL padding (a ``0x00`` between elements).

   If not present, *no padding* is assumed. If the payload data logically
   consists of discrete resources (e.g. Python package resource files), then
   padding applies to these sub-elements as well.

For example, a *blob index* byte sequence of
``0x01 0x02 0x03 0x03 0x0000000000000042 0x04 0x01 0xff 0x00`` would be decoded as:

* ``0x01`` - Start of blob section entry.
* ``0x02 0x03`` - Resource field type definition (``0x02``) for field ``0x03``.
* ``0x03 0x0000000000000042`` - Blob section length (``0x03``) of ``0x42`` bytes
  long.
* ``0x04 0x01`` - Interior padding in blob section (``0x04``) is defined as
  no padding (``0x01``).
* ``0xff`` - End of blob section entry.
* ``0x00`` - End of index.

Resources Index
---------------

Following the *blob index* is the *resources index*.

Each entry in this index defines a sparse set of metadata describing a
single resource.

Entries are composed of a series of ``u8`` identifying pieces of metadata,
followed by field-specific supplementary descriptions.

The following ``u8`` fields and their behavior/payloads are as follows:

``0x00``
   End of index. Special type to denote the end of an index.

``0x01``
   Start of resource entry. Signals the beginning of a new resource. From
   a specification standpoint this isn't strictly required. But it helps ensure
   parser state.

``0x02``
   Previously held the resource *flavor*. This field is deprecated in version 2
   in favor of the individual fields expressing presence of a resource type.
   (See fields starting at ``0x16``.)

``0xff``
   End of resource entry. The next encountered ``u8`` in the index should
   be an *end of index* or *start of resource* marker.

``0x03``
   Resource name. A ``u16`` denoting the length in bytes of the resource name
   immediately follows this byte. The resource name *must* be valid UTF-8.

``0x04``
   Package flag. If encountered, the resource is identified as a Python
   package.

``0x05``
   Namespace package flag. If encountered, the resource is identified as
   a Python *namespace package*.

``0x06``
   In-memory Python module source code. A ``u32`` denoting the length in
   bytes of the module's source code immediately follows this byte.

``0x07``
   In-memory Python module bytecode. A ``u32`` denoting the length in bytes
   of the module's bytecode immediately follows this byte.

``0x08``
   In-memory Python module optimized level 1 bytecode. A ``u32`` denoting the
   length in bytes of the module's optimization level 1 bytecode immediately
   follows this byte.

``0x09``
   In-memory Python module optimized level 2 bytecode. Same as previous,
   except for bytecode optimization level 2.

``0x0a``
   In-memory Python extension module shared library. A ``u32`` denoting the
   length in bytes of the extension module's machine code immediately follows
   this byte.

``0x0b``
   In-memory Python resources data. If encountered, the module/package
   contains non-module resources files and the number of resources is contained in
   a ``u32`` that immediately follows. Following this ``u32`` is an array of
   ``(u16, u64)`` denoting the resource name and payload size for each resource
   in this package.

``0x0c``
   In-memory Python distribution resource. Defines resources accessed from
   ``importlib.metadata`` APIs. If encountered, the module/package contains
   distribution metadata describing the package. The number of files being
   described is contained in a ``u32`` that immediately follows this byte.
   Following this ``u32`` is an array of ``(u16, u64)`` denoting the
   distribution file name and payload size for each virtual file in this
   distribution.

``0x0d``
   In-memory shared library. If set, this resource is a shared
   library and not a Python module. The resource name field is the name of
   this shared library, with file extension (as it would appear in a dynamic
   binary's loader metadata to indicate a library dependency). A ``u64``
   denoting the length in bytes of the shared library data follows. This
   shared library should be loaded from memory.

``0x0e``
   Shared library dependency names. This field indicates the names
   of shared libraries that this entity depends on. The number of library names
   is contained in a ``u16`` that immediately follows this byte. Following this
   ``u16`` is an array of ``u16`` denoting the length of the library name for
   each shared library dependency. Each described shared library dependency
   may or may not be described by other entries in this data structure.

``0x0f``
   Relative filesystem path to Python module source code. A ``u32`` holding
   the length in bytes of a filesystem path encoded in the platform-native file
   path encoding follows. The source code for a Python module will be read from
   a file at this path.

``0x10``
   Relative filesystem path to Python module bytecode. Similar to the
   previous except the filesystem path holds Python module bytecode.

``0x11``
   Relative filesystem path to Python module bytecode at optimization
   level 1. Similar to the previous except for what is being pointed to.

``0x12``
   Relative filesystem path to Python module bytecode at optimization
   level 2. Similar to the previous except for what is being pointed to.

``0x13``
   Relative filesystem path to Python extension module shared library.
   Similar to the previous except the file holds a Python extension module
   loadable as a shared library.

``0x14``
   Relative filesystem path to Python package resources. The number of
   resources is contained in a ``u32`` that immediately follows. Following
   this ``u32`` is an array of ``(u16, u32)`` denoting the resource name and
   filesystem path to each resource in this package.

``0x15``
   Relative filesystem path to Python distribution resources.

   Defines resources accessed from ``importlib.metadata`` APIs. If encountered,
   the module/package contains distribution metadata describing the package.
   The number of files being described is contained in a ``u32`` that
   immediately follows this byte. Following this ``u32`` is an array of
   ``(u16, u32)`` denoting the distribution file name and filesystem path to
   that distribution file.

``0x16``
   Is Python module flag. If set, this resource contains data for
   an importable Python module or package. Resource data is associated with
   Python packages and is covered by this type.

``0x17``
   Is builtin extension module flag. This type represents a Python
   extension module that is built in (compiled into) the interpreter itself
   or is otherwise made available to the interpreter via ``PyImport_Inittab``
   such that it should be imported with the *builtin* importer.

``0x18``
   Is frozen Python module flag. This type represents a Python module
   whose bytecode is *frozen* and made available to the Python interpreter
   via the ``PyImport_FrozenModules`` array and should be imported with the
   *frozen* importer.

``0x19``
   Is Python extension flag. This type represents a compiled Python
   extension. Extensions have specific requirements around how they are to be
   loaded and are differentiated from regular Python modules.

``0x1a``
   Is shared library flag. This type represents a shared library
   that can be loaded into a process.

``0x1b``
   Is utf-8 filename data flag. This type represents an arbitrary filename.
   The resource name is a UTF-8 encoded filename of the file this resource
   represents. The file's data is either embedded in memory or referred to
   via a relative path reference.

``0x1c``
   File data is executable flag.

   If set, the arbitrary file this resource tracks should be marked as
   executable.

``0x1d``
   Embedded file data.

   If present, the resource should be a file resource and this field holds its
   raw file data in memory.

   A ``u64`` containing the length of the embedded data follows this field.

``0x1e``
   UTF-8 relative path file data.

   If present, the resource should be a file resource and this field defines
   the relative path containing that file's data. The relative path filename
   is UTF-8 encoded.

   A ``u32`` denoting the length of the UTF-8 relative path (in bytes) follows.

Blob Sections
-------------

Following the *resources index* is blob data.

Blob data is logically composed of different sections holding data for
different fields for different resources. But there is no internal structure
or separators: all the individual blobs are just laid out next to each other.
The *resources index* for a given field will describe where in a blob
section a particular value occurs.

``pyembed\x01`` Format
----------------------

The initially released/formalized packed resources data format.

Supports resource field types up to and including ``0x15``.

``pyembed\x02`` Format
----------------------

Version 2 of the packed resources data format.

This version introduces field type values ``0x16`` to ``0x1a``. The
resource flavor field type (``0x02``) is deprecated and the individual
field types denoting resource types should be used instead.

(PyOxidizer removed run-time code looking at field type ``0x02`` when
this format was introduced.)

``pyembed\x03`` Format
----------------------

Version 3 of the packed resources data format.

This version introduces field type values ``0x1b`` to ``0x1e``.

These fields provide the ability for a resource to identify itself as
an arbitrary filename and for the arbitrary file data to be embedded
within the data structure or referenced via a relative path.

Unlike previous fields that use OS-native encoding of filesystem
paths (``[u8]`` on POSIX and ``[u16]`` on Windows), the paths for
these new fields use UTF-8. This can't represent all valid paths on
all platforms. But it is portable and works for most paths encountered
in the wild.

Design Considerations
=====================

The design of the packed resources data format was influenced by a handful
of considerations.

Performance is a significant consideration. We want everything to be as fast
as possible. Possible dimensions influencing performance include parse time,
payload size, and I/O access patterns.

The payload is designed such that the *index* data is at the beginning
so a reader only has to read a contiguous slice of data to fully understand
the data within. This is in opposition to jumping around the entire data
structure to extract metadata of the data within. This means that we only
need to page in a fraction of the total backing data structure in order
to initialize our custom importer. In addition, the index data is read
sequentially. Sequential I/O should always be faster than random access
I/O.

x86 is little endian, so we use little endian integers so we don't need
to waste cycles on endian transformation.

We store all data for the same field next to each other in the data
structure. This is in opposition to say packing all of resource A's data
then resource B's, etc. We do this to help maximize locality for similar
data. This can help with performance because often the same field for
multiple resources is accessed together. e.g. an importer will access
a bunch of module bytecode entries at the same time. This locality helps
minimize the number of pages that must be read. Locality can also help
yield higher compression ratios.

Everything is designed to facilitate a reader leveraging 0-copy. If a
reader has the data structure in memory, we don't want to require it
to copy memory in order to reference entries. In Rust speak, we should
be able to hold ``&[u8]`` references everywhere.

There is no checksumming of the data because we don't want to incur
I/O overhead to read the entire blob. It could be added as an optional
feature.

Potential Future Features
=========================

This data structure is robust enough to be used by PyOxidizer to
power importing of every Python module used by a Python interpreter.
However, there are various aspects that could be improved.

Compression
-----------

A potential area for optimization is use of general compression. Various
fields should compress well - either in streaming mode or by utilizing
compression dictionaries. Compression would undermine 0-copy, of course.
But in environments where we want to optimize for size, it could be
desirable.
