// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Specifications

# Packed Modules Data

The custom meta path importer provided by this crate supports importing
Python modules data (source and bytecode) from memory using 0-copy. The
[`PythonConfig`](../struct.PythonConfig.html) simply references a `&[u8]`
(a generic slice over bytes data) providing modules data in a packed format.

The format of this packed data is as follows.

The first 4 bytes are a little endian u32 containing the total number of
modules in this data. Let's call this value `total`.

Following is an array of length `total` with each array element being
a 3-tuple of packed (no interior or exterior padding) composed of 4
little endian u32 values. These values correspond to the module name
length (`name_length`), module source data length (`source_length`),
module bytecode data length (`bytecode_length`), and a `flags` field
to denote special behavior, respectively.

The least significant bit of the `flags` field is set if the
corresponding module name is a package.

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

# Packed Resources Data

The custom meta path importer provided by this crate supports loading
_resource_ data via the `importlib.abc.ResourceReader` interface. Data is
loaded from memory using 0-copy.

Resource file data is embedded in the binary and is represented to
`PythonConfig` as a `&[u8]`.

The format of this packed data is as follows.

The first 4 bytes are a little endian u32 containing the total number
of packages in the data blob. Let's call this value `package_count`.

Following are `package_count` segments that define the resources in each
package. Each segment begins with a pair of little endian u32. The first
integer is the length of the package name string and the 2nd is the number
of resources in this package. Let's call these `package_name_length` and
`resource_count`, respectively.

Following the package header is an array of `resource_count` elements. Each
element is composed of 2 little endian u32 defining the resource's name length
and data size, respectively.

Following this array is the index data for the next package, if there is
one.

After the final package index data is the raw name of the 1st package.
Following it is a vector of strings containing the resource names for that
package. This pattern repeats for each package. All strings MUST be valid
UTF-8. There is no NULL terminator or any other padding between values.

Following the *index* metadata is the raw resource values. Values occur
in the order they were referenced in the index. There is no padding between
values. Values can contain any arbitrary byte sequence.

Example (without literal integer encoding and spaces for legibility):

```text
   2                          # There are 2 packages total.

   (3, 1)                     # Length of 1st package name is 3 and it has 1 resource.
   (3, 42)                    # 1st resource has name length 3 and is 42 bytes long.

   (4, 2)                     # Length of 2nd package name is 4 and it has 2 resources.
   (5, 128)                   # 1st resource has name length 5 and is 128 bytes long.
   (8, 1024)                  # 2nd resource has name length 8 and is 1024 bytes long.

   foo                        # 1st package is named "foo"
   bar                        # 1st resource name is "bar"
   acme                       # 2nd package is named "acme"
   hello                      # 1st resource name is "hello"
   blahblah                   # 2nd resource name is "blahblah"

   foo.bar raw data           # 42 bytes of raw data for "foo.bar".
   acme.hello                 # 128 bytes of raw data for "acme.hello".
   acme.blahblah              # 1024 bytes of raw data for "acme.blahblah"
```

Rationale for the design of this data format is similar to the reasons given
for *Packed Modules Data* above.

*/
