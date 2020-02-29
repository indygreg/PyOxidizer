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
* An index describing each resource and its content.
* A series of blob sections holding the data referenced by the resources
  index.

A resource is composed of various *fields* that describe it. Examples
of fields include the resource name, source code, and bytecode. The index
describes which fields are present and where to find them in the payload.

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
the lengths of various blob sections. Each record is a 2-tuple of `(u8, u64)`
denoting the field ID and the total length of that field's data across all
module entries. There are `blob_sections_count` entries in the index. The
*blob index* ends with the special end of index field. The total number of
bytes in the index included the end of index marker should be
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

## Resource Field Types

The Resources Index allows attributing a sparse set of metadata
with every resource. A `u8` indicates what metadata is being conveyed. Some
field types have additional metadata following this `[u8]` further defining
the field. The values of each defined metadata type follow.

`0x00` - End of index. Special type to denote the end of an index.

`0x01` - Start of resource entry. Signals the beginning of a new resource. From
a specification standpoint this isn't strictly required. But it helps ensure
parser state.

`0x02` - End of resource entry. The next encountered `u8` in the index should
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

`0x0c` - In-memory Python package metadata. Defines resources accessed from
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

Since Rust is the intended target, string data (notably resource names)
are not NULL terminated / C strings because Rust's `str` are not NULL
terminated.

There is no checksumming of the data because we don't want to incur
I/O overhead to read the entire blob. It could be added as an optional
feature.

A potential area for optimization is use of general compression. Various
fields should compress well - either in streaming mode or by utilizing
compression dictionaries. Compression would undermine 0-copy, of course.
But in environments where we want to optimize for size, it could be
desirable.

*/
