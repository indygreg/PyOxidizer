# Oxidized Importer

`oxidized-importer` is a Rust crate that produces a Python extension
module for the ``oxidized_importer`` Python module. This module provides
a Python *meta path finder* that can load Python resources from a pre-built
index, including loading resources from memory. It also exposes functionality
for scanning the filesystem for Python resources, loading those resources
into the custom *meta path finder*, and serializing indexed data into a
binary data structure that can be used for quickly loading an index of
available Python resources.

This project is part of the
[PyOxidized](https://github.com/indygreg/PyOxidizer) project. For more,
see the documentation in the `docs/` directory, rendered online at
https://pyoxidizer.readthedocs.io/en/latest/oxidized_importer.html.
