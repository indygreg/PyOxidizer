// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Specifications

# Embedded Resources Data

The embedded Python interpreter can reference a data blob defining
*resources*. These resources can be consumed by the custom meta path
importer provided by this crate. This meta path importer parses a
serialized data structure at run time, converts it into a Rust data
structure, and uses the discovered resources to influence run-time
behavior.

From a super high level, the embedded resources data structure defines
an iterable of *resources*. A *resource* is an entity with a name,
metadata, and blob fields. The most common *resource* is a Python
module/package. But other resource types (such as shared libraries)
are defined.

The format of this serialized data structure is as follows.

The first 8 bytes is a magic header identifying the content as
our data type and the version of it. The first 7 bytes are `pyembed`
and the following 1 byte denotes a version. The following sections
denote the different magic headers/versions and their semantics.

## `pyembed\x01`

Version 1 of the embedded resources data.

From a high-level, the serialized format consists of:

* A *global header* describing the overall payload.
* An index describing the blob sections present in the payload.
* An index describing each resource and its content.
* A series of blob sections holding the data referenced by the resources
  index.

A resource is composed of various *fields* that describe it. Examples
of fields include the resource name, source code, and bytecode. The resources
index describes which fields are present and where to find them in the payload.

The actual content of fields (e.g. the raw bytes containing source code)
is stored in field-specific sections after the index. Each field has its
own section and data for all resources is stored next to each other. e.g.
you will have all the data for resource names followed by all data for
module sourcecode.

The low-level data format is described below. All integers are
little-endian.

The first 13 bytes after the magic header denote a global header.
The global header consists of:

* A `u8` denoting the number of blob sections, `blob_sections_count`.
* A `u32` denoting the length of the blob index, `blob_index_length`.
* A `u32` denoting the total number of resources in this data,
 `resources_count`.
* A `u32` denoting the length of the resources index,
  `resources_index_length`.

Following the *global header* is the *blob index*. The blob index describes
the various blob sections present in the payload following the *resources
index*.

Each entry in the *blob index* logically consists of a set of fields defining
metadata about each *blob section*. This is encoded by a *start of entry*
`u8` marker followed by N `u8` field type values and their corresponding
metadata, followed by an *end of entry* `u8` marker. The *blob index* is
terminated by an *index of index* `u8` marker. The total number of bytes in
the *blob index* including the *end of index* marker should be
`blob_index_length`.

Following the *blob index* is the *resources index*. Each entry in this index
defines a sparse set of metadata describing a single resource. Entries are
composed of a series of `u8` identifying pieces of metadata, followed by
field-specific supplementary descriptions. For example, a value of `0x02`
denotes the length of the resources's name and is immediately followed by a
`u16` holding said length. See the section below for each field
tracked by this index.

Following the *resources index* is blob data. Blob data is logically consisted
of different sections holding data for different fields for different resources.
But there is no internal structure or separators: all the individual
blobs are just laid out next to each other.

## Blob Field Types

The Blob Index allows attributing a sparse set of metadata with every blob
section entry. The type of metadata being conveyed is defined by a `u8`.
Some field types have additional metadata following that field.

The various field types and their semantics follow.

`0x00` - End of index. This field indicates that there are no more blob
index entries and we've reached the end of the *blob index*.

`0x01` - Start of blob section entry. Encountering this value signals the
beginning of a new blob section. From a specification standpoint, this isn't
strictly required. But it helps ensure parser state.

`0xff` - End of blob section entry. Encountering this value signals the end
of the current blob section definition. The next encountered `u8` in the
index should be `0x01` to denote a new entry or `0x00` to denote end of
index.

`0x02` - Resource field type. This field defines which resource field this
blob section is holding data for. A `u8` following this one will contain
the resource field type value (see section below).

`0x03` - Raw payload length. This field defines the raw length in bytes of
the blob section in the payload. The `u64` containing that length will
immediately follow this `u8`.

`0x04` - Interior padding mechanism. This field defines interior padding
between elements in the blob section. Following this `u8` is another `u8`
denoting the padding mechanism. `0x01` indicates no padding. `0x02` indicates
NULL padding (a `0x00` between elements). If not present, *no padding*
is assumed. If the payload data logically consists of discrete resources
(e.g. Python package resource files), then padding applies to these
sub-elements as well.

## Resource Field Types

The Resources Index allows attributing a sparse set of metadata
with every resource. A `u8` indicates what metadata is being conveyed. Some
field types have additional metadata following this `[u8]` further defining
the field. The values of each defined metadata type follow.

`0x00` - End of index. Special type to denote the end of an index.

`0x01` - Start of resource entry. Signals the beginning of a new resource. From
a specification standpoint this isn't strictly required. But it helps ensure
parser state.

`0x02` - Resource flavor. Declares the type of resource this entry represents.
A `u8` defining the resource flavor immediately follows this byte. See the
section below for valid resource flavors.

`0xff` - End of resource entry. The next encountered `u8` in the index should
be an *end of index* or *start of resource* marker.

`0x03` - Resource name. A `u16` denoting the length in bytes of the resource name
immediately follows this byte. The resource name *must* be valid UTF-8.

`0x04` - Package flag. If encountered, the resource is identified as a Python
package.

`0x05` - Namespace package flag. If encountered, the resource is identified as
a Python *namespace package*.

`0x06` - In-memory Python module source code. A `u32` denoting the length in
bytes of the module's source code immediately follows this byte.

`0x07` - In-memory Python module bytecode. A `u32` denoting the length in bytes
of the module's bytecode immediately follows this byte.

`0x08` - In-memory Python module optimized level 1 bytecode. A `u32` denoting the
length in bytes of the module's optimization level 1 bytecode immediately
follows this byte.

`0x09` - In-memory Python module optimized level 2 bytecode. Same as previous,
except for bytecode optimization level 2.

`0x0a` - In-memory Python extension module shared library. A `u32` denoting the
length in bytes of the extension module's machine code immediately follows
this byte.

`0x0b` - In-memory Python resources data. If encountered, the module/package
contains non-module resources files and the number of resources is contained in
a `u32` that immediately follows. Following this `u32` is an array of
`(u16, u64)` denoting the resource name and payload size for each resource
in this package.

`0x0c` - In-memory Python distribution resource. Defines resources accessed from
`importlib.metadata` APIs. If encountered, the module/package contains
distribution metadata describing the package. The number of files being
described is contained in a `u32` that immediately follows this byte.
Following this `u32` is an array of `(u16, u64)` denoting the distribution
file name and payload size for each virtual file in this distribution.

`0x0d` - In-memory shared library. If set, this resource is a shared
library and not a Python module. The resource name field is the name of
this shared library, with file extension (as it would appear in a dynamic
binary's loader metadata to indicate a library dependency). A `u64`
denoting the length in bytes of the shared library data follows. This
shared library should be loaded from memory.

`0x0e` - Shared library dependency names. This field indicates the names
of shared libraries that this entity depends on. The number of library names
is contained in a `u16` that immediately follows this byte. Following this
`u16` is an array of `u16` denoting the length of the library name for
each shared library dependency. Each described shared library dependency
may or may not be described by other entries in this data structure.

`0x0f` - Relative filesystem path to Python module source code. A `u32` holding
the length in bytes of a filesystem path encoded in the platform-native file
path encoding follows. The source code for a Python module will be read from
a file at this path.

`0x10` - Relative filesystem path to Python module bytecode. Similar to the
previous except the filesystem path holds Python module bytecode.

`0x11` - Relative filesystem path to Python module bytecode at optimization
level 1. Similar to the previous except for what is being pointed to.

`0x12` - Relative filesystem path to Python module bytecode at optimization
level 2. Similar to the previous except for what is being pointed to.

`0x13` - Relative filesystem path to Python extension module shared library.
Similar to the previous except the file holds a Python extension module
loadable as a shared library.

`0x14` - Relative filesystem path to Python package resources. The number of
resources is contained in a `u32` that immediately follows. Following this
`u32` is an array of `(u16, u32)` denoting the resource name and filesystem
path to each resource in this package.

`0x15` - Relative filesystem path to Python distribution resources.
Defines resources accessed from `importlib.metadata` APIs. If encountered,
the module/package contains distribution metadata describing the package.
The number of files being described is contained in a `u32` that immediately
follows this byte. Following this `u32` is an array of `(u16, u32)` denoting
the distribution file name and filesystem path to that distribution file.

## Resource Flavors

The data format allows defining different types/flavors of resources.
This flavor of a resource is identified by a `u8`. The declared flavors are:

`0x00` - No flavor. Should not be encountered.

`0x01` - Python module/package. This type represents a normal Python
module.

`0x02` - Builtin Python extension module. This type represents a Python
extension module that is built in (compiled into) the interpreter itself
or is otherwise made available to the interpreter via `PyImport_Inittab`
such that it should be imported with the *builtin* importer.

`0x03` - Frozen Python module. This type represents a Python module whose
bytecode is *frozen* and made available to the Python interpreter via the
`PyImport_FrozenModules` array and should be imported with the *frozen*
importer.

`0x04` - Python extension. This type represents a compiled Python extension.
Extensions have specific requirements around how they are to be loaded and
are differentiated from regular Python modules.

`0x05` - Shared library. This type represents a shared library that can be
loaded into a process.

## Design Considerations

The design of the embedded resources data format was influenced by a handful
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
be able to hold `&[u8]` references everywhere.

There is no checksumming of the data because we don't want to incur
I/O overhead to read the entire blob. It could be added as an optional
feature.

A potential area for optimization is use of general compression. Various
fields should compress well - either in streaming mode or by utilizing
compression dictionaries. Compression would undermine 0-copy, of course.
But in environments where we want to optimize for size, it could be
desirable.

*/
