# PyOxidizer

[![Build Status](https://dev.azure.com/gregoryszorc/PyOxidizer/_apis/build/status/indygreg.PyOxidizer?branchName=main)](https://dev.azure.com/gregoryszorc/PyOxidizer/_build/latest?definitionId=1&branchName=main)

`PyOxidizer` is a utility for producing binaries that embed Python.
The over-arching goal of `PyOxidizer` is to make complex packaging and
distribution problems simple so application maintainers can focus on
building applications instead of toiling with build systems and packaging
tools.

`PyOxidizer` is capable of producing a single file executable - with
a copy of Python and all its dependencies statically linked and all
resources (like `.pyc` files) embedded in the executable. You can
copy a single executable file to another machine and run a Python
application contained within. It *just works*.

`PyOxidizer` exposes its lower level functionality for embedding
self-contained Python interpreters as a tool and software library. So if
you don't want to ship executables that only consist of a Python
application, you can still use `PyOxidizer` to e.g. produce a library
containing Python suitable for linking in any application or use
`PyOxidizer`'s embedding library directly for embedding Python in a
larger application.

The _Oxidizer_ part of the name comes from Rust: executables produced
by `PyOxidizer` are compiled from Rust and Rust code is responsible
for managing the embedded Python interpreter and all its operations.
If you don't know Rust, that's OK: PyOxidizer tries to make the existence
of Rust nearly invisible to end-users.

While solving packaging and distribution problems is the primary goal
of `PyOxidizer`, a side-effect of solving that problem with Rust is
that `PyOxidizer` can serve as a bridge between these two languages.
`PyOxidizer` can be used to easily add a Python interpreter to _any_
Rust project. But the opposite is also true: `PyOxidizer` can also be
used to add Rust to Python. Using `PyOxidizer`, you could _bootstrap_
a new Rust project which contains an embedded version of Python and your
application. Initially, your project is a few lines of Rust that
instantiates a Python interpreter and runs Python code. Over time,
functionality could be (re)written in Rust and your previously
Python-only project could leverage Rust and its diverse ecosystem. Since
`PyOxidizer` abstracts the Python interpreter away, this could all be
invisible to end-users: you could rewrite an application from Python to
Rust and people may not even know because they never see a `libpython`,
`.py` files, etc.

## Project Info

:house: The official home of the `PyOxidizer` project is
https://github.com/indygreg/PyOxidizer.

:notebook_with_decorative_cover: Documentation (generated from the `docs/` directory) is available
at https://pyoxidizer.readthedocs.io/en/latest/index.html.

:speech_balloon: The [pyoxidizer-users](https://groups.google.com/forum/#!forum/pyoxidizer-users)
mailing list is a forum for users to discuss all things PyOxidizer.

:moneybag: If you want to financially contribute to PyOxidizer, do so
[on Patreon](https://www.patreon.com/indygreg).
