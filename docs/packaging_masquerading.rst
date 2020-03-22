.. _packaging_masquerading:

=====================================
Masquerading As Other Packaging Tools
=====================================

Tools to package and distribute Python applications existed several
years before ``PyOxidizer``. Many Python packages have learned to perform
special behavior when the _fingerprint* of these tools is detected at
run-time.

First, ``PyOxidizer`` has its own fingerprint: ``sys.oxidized = True``. The
presence of this attribute can indicate an application running with
``PyOxidizer``. Other applications are discouraged from defining this
attribute.

Since ``PyOxidizer``'s run-time behavior is similar to other packaging
tools, ``PyOxidizer`` supports falsely identifying itself as these other
tools by emulating their fingerprints.

The ``EmbbedPythonConfig`` configuration section defines the
boolean flag ``sys_frozen`` to control whether ``sys.frozen = True``
is set. This can allow ``PyOxidizer`` to advertise itself as a *frozen*
application.

In addition, the ``sys_meipass`` boolean flag controls whether a
``sys._MEIPASS = <exe directory>`` attribute is set. This allows
``PyOxidizer`` to masquerade as having been built with PyInstaller.

.. warning::

   Masquerading as other packaging tools is effectively lying and can
   be dangerous, as code relying on these attributes won't know if
   it is interacting with ``PyOxidizer`` or some other tool. It is
   recommended    to only set these attributes to unblock enabling
   packages to work with ``PyOxidizer`` until other packages learn to
   check for ``sys.oxidized = True``. Setting ``sys._MEIPASS`` is
   definitely the more risky option, as a case can be made that
   PyOxidizer should set ``sys.frozen = True`` by default.
