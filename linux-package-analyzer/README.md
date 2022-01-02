# linux-package-analyzer

`linux-package-analyzer` is a binary Rust crate providing the `lpa` command-line
executable. This CLI tool facilitates indexing and then inspecting the contents of
Linux package repositories. Both Debian and RPM based repositories are supported.

Run `lpa help` for more details.

# Installing

```
# From the latest commit in the canonical Git repository:
$ cargo install --git https://github.com/indygreg/PyOxidizer linux-package-analyzer

# From the root directory of a Git source checkout:
$ cargo install --path linux-package-analyzer
```

# How It Works

`lpa` exposes sub-commands for importing the contents of a specified package
repository into a local SQLite database. Essentially, the package lists from
the remote repository are retrieved and referenced packages are downloaded
and their content indexed. The indexed content includes:

* Files installed by the package
* ELF file content
 * File header values
 * Section metadata
 * Dynamic library dependencies
 * Symbols
 * x86 instruction counts

Additional sub-commands exist for performing analysis of the indexed content
within the SQLite databases. However, there is a lot of data in the SQLite
database that is not exposed or queryable via the CLI.

# Example

The following command will import all packages from Ubuntu 21.10 Impish for
amd64 into the SQLite database `ubuntu-impish.db`:

```
lpa --db ubuntu-impish.db \
    import-debian-repository \
    --components main,multiverse,restricted,universe \
    --architectures amd64 \
    http://us.archive.ubuntu.com/ubuntu impish
```

This should download ~96 GB of packages (as of January 2022) and create a
~12 GB SQLite database.

Once we have a populated database, we can run commands to query its content.

To see which files import (and presumably call) a specific C function:

```
lpa --db ubuntu-impish.db \
    elf-files-importing-symbol OPENSSL_init_ssl
```

To see what are the most popular ELF section names:

```
lpa --db ubuntu-impish.db elf-section-name-counts
```

Power users may want to write their own queries against the database. To
get started, open the SQLite database and poke around:

```
$ sqlite3 ubuntu-impish.db
SQLite version 3.35.5 2021-04-19 18:32:05
Enter ".help" for usage hints.

sqlite> .tables
elf_file                          package_file
elf_file_needed_library           symbol_name
elf_file_x86_base_register_count  v_elf_needed_library
elf_file_x86_instruction_count    v_elf_symbol
elf_file_x86_register_count       v_package_elf_file
elf_section                       v_package_file
elf_symbol                        v_package_instruction_count
package

sqlite> select * from v_elf_needed_library where library_name = "libc.so.6" order by package_name asc limit 1;
0ad|0.0.25b-1|http://us.archive.ubuntu.com/ubuntu/pool/universe/0/0ad/0ad_0.0.25b-1_amd64.deb|usr/games/pyrogenesis|libc.so.6
```

The `v_` prefixed tables are views and conveniently pull in data from
multiple tables. For example, `v_elf_symbol` has all the columns of
`elf_symbol` but also expands the package name, version, file path, etc.

# Constants and Special Values

Various ELF data uses constants to define attributes. e.g. `elf_file.machine`
is an integer holding the ELF machine type. A good reference for values of
these constants is
https://docs.rs/object/0.28.2/src/object/elf.rs.html#1-6256.

`lpa` also exposes various `reference-*` commands for printing known
values.

# Known Issues

## x86 Disassembly Quirks

On package index/import, an attempt is made to disassemble x86 / x86-64 files so
instruction counts and register usage can be stored in the database.

We disassemble all sections marked as executable. Instructions in other
sections may not be found (this is hopefully rare).

We disassemble using the [iced_x86](https://crates.io/crates/iced-x86) Rust crate.
So any limitations in that crate apply to the disassembler.

We disassemble instructions by iterating over content of the binary section,
attempting to read instructions until end of section. Executable sections can
contain NULL bytes, inline data, and other bytes that may not represent valid
instructions. This will result in many byte sequences decoding to the special
*invalid* instruction. In some cases, a byte sequence may decode to an
instruction even though the underlying data is not an instruction. i.e. there
can be false positives on instruction counts.

## Intermittent HTTP Failures on Package Retrieval

Intermittent HTTP GET failures when importing packages is expected due to
intrinsic network unreliability. This often manifests as an error like the
following:

```
error processing package (ignoring): repository I/O error on path pool/universe/g/gcc-10/gnat-10_10.3.0-11ubuntu1_amd64.deb: Custom { kind: Other, error: "error sending HTTP request: reqwest::Error { kind: Request, url: Url { scheme: \"http\", cannot_be_a_base: false, username: \"\", password: None, host: Some(Domain(\"us.archive.ubuntu.com\")), port: None, path: \"/ubuntu/pool/universe/g/gcc-10/gnat-10_10.3.0-11ubuntu1_amd64.deb\", query: None, fragment: None }, source: hyper::Error(IncompleteMessage) }" }
```

If you see failures like this, simply retry the import operation. Already
imported packages should automatically be skipped.

## Package Server Throttling

`lpa` can issue parallel HTTP requests to retrieve content. By default, it
issues up to as many parallel requests as CPU cores/threads.

Some package repositories limit the number of simultaneous HTTP
connections/requests by client. If your machine has many CPU cores, you may run
into these limits and get a high volume of HTTP errors when fetching packages.
To mitigate, reduce the number of simultaneous I/O operations via `--threads`.
e.g. `lpa --threads 4 ...`

## SQLite Integrity Weakening

To maximize speed of import operations, SQLite databases have their content
integrity and durability guarantees weakened via `PRAGMA` statements issued
on database open. A process or machine crash during a write operation could
corrupt the SQLite database more easily than it otherwise would.

# Project Relationship

`linux-package-analyzer` is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcomed
and encouraged!
