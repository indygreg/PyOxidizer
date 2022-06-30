.. _pyoxy:

===========================
The ``PyOxy`` Python Runner
===========================

PyOxy is:

* A single file Python distribution (no temporary files or virtual filesystems
  like SquashFS: everything is imported directly from data inside the
  executable).
* An alternative implementation and re-imagination of the ubiquitous ``python``
  command, enabling you to have nearly full control over how to run a Python
  interpreter.
* Written in Rust, using reusable components initially built for PyOxidizer.
* Part of the PyOxidizer umbrella project.

The official home of PyOxy is https://github.com/indygreg/PyOxidizer/. Read the
(`stable <https://gregoryszorc.com/docs/pyoxy/stable/>`_ |
`latest <https://gregoryszorc.com/docs/pyoxy/main/>`_) docs online.

Releases can be found at https://github.com/indygreg/PyOxidizer/releases.

.. toctree::
   :maxdepth: 2

   pyoxy_overview
   pyoxy_installing
   pyoxy_yaml
   pyoxy_interpreter_config
   pyoxy_developing
   pyoxy_history
