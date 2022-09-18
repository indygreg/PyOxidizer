.. _tugger_overview:

========
Overview
========

Tugger aims to be a generic tool to help application maintainers ship their
applications to end-users.

Tugger can be thought of a specialized build system for distributable
artifacts (Windows MSI installers, Debian packages, RPMs, etc). However,
Tugger itself is generally not concerned with details of how a particular
file is built: Tugger's role is to consume existing files and *package* them
into artifacts that are distributed/installed on other machines.

Designed to Be Platform Agnostic
================================

An explicit goal of Tugger is to be platform agnostic and to have as much
functionality implemented in-process. For example, it should be possible to
produce a Linux ``.deb`` from Windows, a Windows MSI installer from macOS, or
a macOS DMG from Linux without any out-of-process dependencies.

Tugger attempts to implement packaging functionality in Rust with minimal
dependence on external tools. For example, RPMs and Debian packages are built
by constructing the raw archive files using Rust code rather than calling out
to tools like ``rpmbuild`` or ``debuild``. This enables Tugger to build
artifacts that don't target the current architecture or operating system.

While Tugger may not achieve this goal for all distributable formats and
architectures, it is something that Tugger strives to do.

File Centric View
=================

Tugger attempts to take a file-centric view towards packaging. This helps
achieve platform independent and *cross-compiling*. What this means in
practice is many of Tugger's packaging facilities operate by taking an
input set of files and assembling them into some other distributable format.
Contrast this with specialized tools for each distributable format, which
generally invoke a custom build system and have domain-specific configuration
files.

A side-effect of this decision is that Tugger is often not aware of build
systems: it is often up to you to script Tugger to produce the files you
wish to distribute.

.. _tugger_crates:

Modular Crate Architecture
==========================

Tugger is composed of a series - a *fleet* if you will - of Rust crates.
Each Rust crate provides domain-specific functionality. While the Rust
crates are part of the Tugger project, an attempt is made to implement
them such that they can be used outside of Tugger.

The following crates compose Tugger's crate *fleet*:

``tugger-binary-analysis``
   Analyze platform native binaries. Finds library dependencies. Identifies
   Linux distribution compatibility. Etc.

``tugger-common``
   Shared functionality required by multiple crates. This entails things
   like downloading files, shared test code, etc.

``tugger-debian``
   Debian packaging primitives. Parsing and serializing control files.
   Writing ``.deb`` files.

``tugger-rpm``
   RPM packaging primitives.

``tugger-snapcraft``
   Snapcraft packaging. Represent ``snapcraft.yaml`` files. Invoke
   ``snapcraft`` to produce ``.snap`` files.

``tugger-windows``
   Windows-specific functionality. Finding the Microsoft SDK and Visual C++
   Redistributable files. Signing Windows binaries.

``tugger-wix``
   Interface to the WiX Toolset (produces Windows ``.msi`` and ``.exe``
   installers). Can build Windows installers with little-to-no knowledge
   about how the WiX Toolset works.

``tugger``
   The primary crate. Implements Starlark dialect and driver code for
   running it. This crate has minimal use as a library, as most library
   functionality is within the domain-specific crates.
