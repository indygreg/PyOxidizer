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

An explicit goal of Tugger is to be platform agnostic where possible. For
example, it should be possible to produce a Linux RPM from Windows, a Windows
MSI installer from macOS, or a macOS DMG from Linux. While Tugger may not
achieve this goal for all distributable formats and architectures, it is
something that Tugger strives to do.

Tugger attempts to take a file-centric view towards packaging. This helps
achieve platform independent and *cross-compiling*. What this means in
practice is many of Tugger's packaging facilities operate by taking an
input set of files and assembling them into some other distributable format.
Contrast this with specialized tools for each distributable format, which
generally invoke a custom build system and have domain-specific configuration
files.

Tugger attempts to implement packaging functionality in Rust with minimal
dependence on external tools. For example, RPMs and Debian packages are built
by constructing the raw archive files using Rust code rather than calling out
to tools like ``rpmbuild`` or ``debuild``. This enables Tugger to build
artifacts that don't target the current architecture or operating system.
