// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian packaging primitives.

This crate defines pure Rust implementations of Debian packaging primitives. Debian packaging
(frequently interacted with by tools like `apt` and `apt-get`) provides the basis for
packaging on Debian-flavored Linux distributions like Debian and Ubuntu.

The canonical home of this crate is <https://github.com/indygreg/PyOxidizer>. Please file issues
and pull requests there.

# Goals

## Compliance and Compatibility

We want this crate to be as-compliant and as-compatible as possible with in-the-wild Debian
packaging deployments so it can be used as a basis to implementing tools which consume and
produce entities that are compatible with the official Debian packaging implementations.

This crate could be considered an attempt to reimplement aspects of
[apt](https://salsa.debian.org/apt-team/apt) in pure Rust. (The `apt` repository defines
command line tools like `apt-get` as well as libraries like `libapt-pkg` and `libapt-inst`.
This crate is more focused on providing the library-level interfaces. However, a goal is
to have as much code be usable as a library so the surface area of any tools is minimal.)

## Determinism and Reproducibility

To help combat the rise in software supply chain attacks and to make debugging and testing
easier, a goal of this crate is to be as deterministic and reproducible as possible.

Given the same source code / version of this crate, operations like creating a `.deb` file
or building a repository from indexed `.deb` files should be as byte-for-byte identical as
reasonably achievable.

## Performance

We strive for highly optimal implementations of packaging primitives wherever possible.

We want to facilitate intensive operations (like reading all packages in a Debian
repository) or publishing to a repository to scale out to as many CPU cores as possible.

Read operations like parsing control files or version strings should be able to use
0-copy to avoid excessive memory allocations and copying.

# A Tour of Functionality

A `.deb` file defines a Debian package. Readers and writers of `.deb` files exist in the
[deb] module. To read the contents of a `.deb` defining a binary package, use
[deb::reader::BinaryPackageReader]. To create new `.deb` files, use [deb::builder::DebBuilder].

A common primitive within Debian packaging is *control files*. These consist of *paragraphs*
of key-value metadata. Low-level control file primitives are defined in the [control] module.
[control::ControlParagraph] defines a paragraph, which consists of [control::ControlField].
[control::ControlFile] provides an interface for a *control file*, which consists of multiple
paragraphs. [control::ControlParagraphReader] implements a streaming reader of control files
and [control::ControlParagraphAsyncReader] implements an asynchronous streaming reader.

There are different flavors of *control files* within Debian packaging.
[binary_package_control::BinaryPackageControlFile] defines a *control file* for a binary package.
This type provides helper functions for resolving common fields on binary control files.

There is a meta language for expressing dependencies between Debian packages. The
[dependency] module defines types for parsing and writing this language. e.g.
[dependency::DependencyList] represents a parsed list of dependencies like
`libc6 (>= 2.4), libx11-6`. [dependency::PackageDependencyFields] represents a collection
of control fields that define relationships between packages.

The [package_version] module implements Debian package version string parsing,
serialization, and comparison. [package_version::PackageVersion] is the main type used for this.

The [dependency_resolution] module implements functionality related to resolving dependencies.
e.g. [dependency_resolution::DependencyResolver] can be used to index known binary packages
and find direct and transitive dependencies. This could be used as the basis for a package
manager or other tool wishing to walk the dependency tree for a given package.

The [repository] module provides functionality related to Debian repositories, which are
publications of Debian packages and metadata. The [repository::RepositoryRootReader] trait
provides an interface for reading the root directory of a repository and
[repository::ReleaseReader] provides an interface for reading content from a parsed
`[In]Release` file. The [repository::RepositoryWriter] trait abstracts I/O for writing
to a repository. Repository interaction involves many support primitives.
[repository::release::ReleaseFile] represents an `[In]Release` file. Support for verifying
PGP signatures is provided. [repository::contents::ContentsFile] represents a `Contents`
file.

Concrete implementations of repository interaction exist. [repository::http::HttpRepositoryClient]
enables reading from an HTTP-hosted repository (e.g. `http://archive.canonical.com/ubuntu`).
[repository::filesystem::FilesystemRepositoryWriter] enables writing repositories to a local
filesystem.

The [repository::builder] module contains functionality for creating and publishing
Debian repositories. [repository::builder::RepositoryBuilder] is the main type for
publishing Debian repositories.

The [signing_key] module provides functionality related to PGP signing.
[signing_key::DistroSigningKey] defines PGP public keys for well-known signing keys used by
popular Linux distributions. [signing_key::signing_secret_key_params_builder()] and
[signing_key::create_self_signed_key()] enable easily creating signing keys for Debian
repositories.

Various other modules provide miscellaneous functionality. [io] defines I/O helpers, including
stream adapters for validating content digests on read and computing content digests on write.
[pgp] contains helpers for interacting with PGP primitives.

# Crate Features

The optional and enabled-by-default `http` feature enables HTTP client support for interacting
with Debian repositories via HTTP.
*/

pub mod binary_package_control;
pub mod binary_package_list;
pub mod changelog;
pub mod control;
pub mod deb;
pub mod dependency;
pub mod dependency_resolution;
pub mod error;
pub mod io;
pub mod package_version;
pub mod pgp;
pub mod repository;
pub mod signing_key;
pub mod source_package_control;
