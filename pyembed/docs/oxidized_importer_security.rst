.. _oxidized_importer_security:

==========================================
Security Implications of Loading Resources
==========================================

``OxidizedFinder`` allows Python code to define its own ``OxidizedResource``
instances to be made available for loading. This means Python code can define
its own Python module source or bytecode that could later be executed. It also
allows registration of extension modules and shared libraries, which give
a vector for allowing execution of native machine code.

This feature has security implications, as it provides a vector for arbitrary
code execution.

While it might be possible to restrict this feature to provide stronger
security protections, we have not done so yet. Our thinking here is that
it is extremely difficult to sandbox Python code. Security sandboxing at the
Python layer is effectively impossible: the only effective mechanism to
sandbox Python is to add protections at the process level. e.g. by restricting
what system calls can be performed. We feel that the capability to inject
new Python modules and even shared libraries via ``OxidizedFinder`` doesn't
provide any new or novel vector that doesn't already exist in Python's standard
library and can't already be exploited by well-crafted Python code. Therefore,
this feature isn't a net regression in security protection.

If you have a use case that requires limiting the features of
``OxidizedFinder`` so security isn't sacrificed, please
`file an issue <https://github.com/indygreg/PyOxidizer/issues>`.
