.. _apple_codesign_quirks:

============================
Known Issues and Limitations
============================

Apple code signing is complex. While this project strives to provide
all the features and compatibility that Apple's official tooling provides,
we won't always get it right. This document captures some of the areas where
we know we fall short.

Bundle Handling in General
==========================

Bundle signing is complex for a few reasons:

* The types and layouts of bundles are highly varied. Application bundles.
  Frameworks. Kernel extensions. macOS flavored vs iOS flavored bundles. The
  list goes on.
* Bundles can be nested.
* Signatures in nested bundles often need to propagate to their parent bundle.
* Bundles encapsulate other signable entities, notably Mach-O binaries.

All this complexity means bundle signing is susceptible to a lot of subtle
bugs and variation from how Apple's tooling does it.

If you find bugs in bundle signing or have suggestions for improving its
ergonomics, please `file a GitHub issue <https://github.com/indygreg/PyOxidizer/issues/new>`_!

Cannot Sign File Contents of DMGs
=================================

We support signing DMGs. But we can't recursively inspect the files within
DMGs and sign those. e.g. if a DMG contains a Mach-O binary, we can't
sign that Mach-O by unpacking it from the DMG and writing a new DMG.

The reason we can't do this is because DMGs contain a nested filesystem
(likely HFS+) and we don't (yet) have a cross-platform mechanism for reading
and writing HFS+ filesystems.

On macOS, we could call out to ``hdiutil`` to mount a DMG to see its
contents and again to create a new DMG. However, this isn't implemented
because we don't perceive there to be value in it: if you have access to
macOS you should probably just use Apple's official signing tooling!

There are open source libraries for reading and writing HFS+ filesystems.
We could potentially integrate those to support reading and writing the
contents of DMGs. We could also potentially leverage a pure Rust HFS+
implementation (this is a preferred solution).

DMG also supports multiple embedded filesystem types and it is possible
we could leverage one that isn't HFS+ (or APFS) and produce working DMGs.
This is an area we haven't yet explored.

If you want to distribute DMGs signed with this tool that themselves have
signed files, you'll need to sign the files inside the DMG before the DMG
is created. Then you'll need to create the DMG (using ``hdiutil`` or
whatever tool you have access to) then feed that DMG into this tool for
signing.

https://github.com/indygreg/PyOxidizer/issues/540 is our tracking issue
for DMG writing support. If you have ideas, please comment there!

Cannot Recursively Sign Flat Packages (``.pkg`` Installers)
===========================================================

Flat Packages (``.pkg`` installers) are a complex file format.

We have support for signing ``.pkg`` installers by reading the files
within a flat package. And we are capable of recursively extracting
and signing the ``.pkg`` installers that themselves are often embedded
in ``.pkg`` installers.

What we don't yet have support for is mutating the file content within
flat packages / ``.pkg`` installers. This means we can't recursively sign
nested ``.pkg`` installers or bundles or Mach-O binaries within.

The main blocker to implementing ``.pkg`` writing is support for
reading and writing Apple's *Bill of Materials* file format. These are
the ``Bom`` files within flat packages. The author of this project
has an unpublished Rust crate to read and write bom files but he
encountered issues getting it to write files that validate with Apple's
implementation.

So if you want to sign ``.pkg`` files that themselves containable signable
entities, you need to sign files going into the ``.pkg`` before creating
the ``.pkg``. Then you need to create the ``.pkg`` and invoke this tool to
sign the ``.pkg``. For installers that contained nested ``.pkg`` installers,
this process will be quite tedious. Invoking ``componentbuild`` and
``productbuild`` will likely be much simpler.

https://github.com/indygreg/PyOxidizer/issues/541 is our tracking issue
for flat packages writing support.

Extra Signing or Time-Stamp Token Operations
============================================

Signatures often need to encapsulate the size of the resulting signature.
This creates a chicken-and-egg problem because how can we know the size of
the resulting signature before we actually produce it!

In some cases, this tool will create a *fake* signature and obtain an
actual time-stamp token from a server in order to resolve the size of
the data so we can better estimate the size of the real signature.

We are not sure if Apple's tooling does this. But ours does and the
extra operations can be annoying because they may require extra unlocks
of signing keys or communications with a time-stamp token server.

We can likely eliminate the extra use of the signing key for generating
these stand-in signatures and we can probably only make 1 request to the
time-stamp token server to obtain the size of its signatures. But we
haven't implemented this throughout the code base yet.

https://github.com/indygreg/PyOxidizer/issues/542 and
https://github.com/indygreg/PyOxidizer/issues/543 track improvements here.

Long Tail of Random Discrepancies from Apple's Tooling
======================================================

Apple's code signature format is really, really complex. There are tons of
data structures and fields with complex values.

There is likely a long tail of minor differences in implementation that
result in variations between the behavior of our implementation and Apple's.

In general, we consider differences in behavior in our implementation to
be bugs worth filing. Please follow the instructions at
:ref:`apple_codesign_debugging` to file GitHub issues with meaningful
details to debug the differences!

Known areas where discrepancies are likely include:

* The *code requirements* expression embedded into Mach-O binaries. We attempt
  to derive one based on the signing key. The expression may not be exactly what
  Apple's tools derive automatically. We consider this a bug.
* Executable segment flags and code signing flags. The exact logic for
  determining what flags to set when is complex. In general, we consider
  differences in behavior here to be bugs.
* Size of embedded signatures. You often need to estimate the size of the produced
  embedded signature before signing because the signature encapsulates its own
  size. Our estimation method varies from Apple's and can result in signatures
  with more or less padded null bytes. This difference should be mostly harmless.
  Improvements to make our signatures use fewer wasteful extra padding are
  appreciated.
