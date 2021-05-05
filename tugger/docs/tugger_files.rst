.. py:currentmodule:: starlark_tugger

.. _tugger_files:

==================
Working with Files
==================

Tugger's Starlark dialect exposes various types and functions for
working with files. The most important primitives are:

:py:class:`FileContent`
   Represents an individual file - it's filename, content, and an executable bit.

:py:class:`FileManifest`
   Represents a collection of files. This is a glorified mapping from an install
   path to :py:class:`FileContent`.

:py:func:`glob`
   Read files from the filesystem by performing a *glob* filename pattern
   search.

If a primitive in Tugger is tracking a logical collection of files (e.g.
a :py:class:`WiXInstaller` tracking files that an installer should
materialize), chances are that it is using a :py:class:`FileManifest` for
doing so.

Copying Files
=============

Say you want to collect and then materialize a collection of files.
Here's how you would do that in Starlark.

.. code-block:: python

    # Create a new empty file manifest.
    m = FileManifest()

    # Add individual files to the manifest.
    m.add_file(FileContent(path = "file0.txt"))
    m.add_file(FileContent(path = "file1.txt"))

    # Then copy/materialize them somewhere.
    m.install("output/directory")

If you wanted, you could even rename files as part of this:

.. code-block:: python

    m = FileManifest()

    f = FileContent(path = "file0.txt")
    f.filename = "renamed.txt"
    m.add_file(f)

Or more concisely:

.. code-block:: python

    m = FileManifest()
    m.add_file(f, path="renamed.txt")
