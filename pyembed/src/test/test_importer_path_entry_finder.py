# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
from __future__ import annotations

from contextlib import contextmanager
from importlib.machinery import PathFinder
import marshal
import os
from pathlib import Path
import re
import sys
import tempfile
from typing import Iterable, Optional, Tuple, Union, TYPE_CHECKING
import unittest
from unittest.mock import patch

from oxidized_importer import OxidizedFinder, OxidizedResource

if TYPE_CHECKING:
    import importlib.abc
    from importlib.machinery import ModuleSpec


def make_finder(*modules: Tuple[str, str, bool]) -> OxidizedFinder:
    """Create an ``OxidizedFinder`` with modules defined by ``modules``.

    ``modules`` must be tuples of the form (name, source_code, is_package).
    """
    mpf = OxidizedFinder()
    for module_name, source, is_pkg in modules:
        # See example in OxidizedFinder.add_resource
        resource = OxidizedResource()
        resource.is_module = True
        resource.name = module_name
        resource.is_package = is_pkg
        resource.in_memory_source = source.encode("utf-8")
        resource.in_memory_bytecode = marshal.dumps(compile(
            source, module_name, "exec"))
        mpf.add_resource(resource)
    return mpf


@contextmanager
def chdir(dir: Union[str, bytes, os.PathLike]) -> Iterable[Path]:
    "Change the current directory to ``dir``, yielding the previous one."
    old_cwd = Path.cwd()
    try:
        os.chdir(dir)
        yield old_cwd
    finally:
        os.chdir(old_cwd)


class TestImporterPathEntryFinder(unittest.TestCase):

    def finder(
        self,
        path: Union[str, bytes, os.PathLike],
        package: str,
    ) -> importlib.abc.PathEntryFinder:
        """Add the following package hierarchy to the returned finder:

        - ``a`` imports ``a.b`` imports ``a.b.c``
        - ``one`` imports ``three`` from ``.two``; ``one.two`` imports
          ``one.two.three``.
        - ``on``, ``on.tשo``, and ``on.tשo.۳`` each pass
        """
        mpf = make_finder(
            ("a", "import a.b", True),
            ("a.b", "import a.b.c", True),
            ("a.b.c", "pass", False),

            ("one", "from .two import three", True),
            ("one.two", "import one.two.three", True),
            ("one.two.three", "pass", False),

            ("on", "pass", True),
            ("on.tשo", "pass", True),
            ("on.tשo.۳", "pass", False),
        )
        pef = mpf.path_hook(path)
        self.assertEqual(pef._package, package)
        self.assertRaises(AttributeError, setattr, pef, "_package", package)
        return pef

    def assert_spec(
        self,
        spec: ModuleSpec,
        name: str,
        is_pkg: bool,
        Loader: importlib.abc.Loader = OxidizedFinder,
        origin: Optional[str] = None
    ) -> None:
        self.assertIsNotNone(spec, name)
        self.assertEqual(spec.name, name, spec)
        self.assertTrue(
            isinstance(spec.loader, Loader) or issubclass(spec.loader, Loader),
            spec)
        self.assertEqual(spec.origin, origin, spec)
        self.assertIsNone(spec.cached, spec)
        self.assertFalse(spec.has_location, spec)
        if is_pkg:
            self.assertIsNotNone(spec.submodule_search_locations, spec)
            for entry in spec.submodule_search_locations:
                self.assertIsInstance(entry, (str, bool), spec)
            self.assertEqual(spec.parent, name, spec)
        else:
            self.assertIsNone(spec.submodule_search_locations, spec)
            self.assertEqual(spec.parent, name.rpartition(".")[0], spec)

    def assert_find_spec_nested(self, path: Union[str, bytes, os.PathLike]) -> None:
        finder = self.finder(path, "on")
        # Return None for modules outside the search path, even if their names
        # are prefixed by the path.
        self.assertIsNone(finder.find_spec("one.two"))
        # Return None for modules shallower than the search path
        self.assertIsNone(finder.find_spec("on"))
        # Return None for modules deeper than the search path
        self.assertIsNone(finder.find_spec("on.tשo.۳"))
        # Return a correct ModuleSpec for modules in the search path
        self.assert_spec(finder.find_spec("on.tשo"), "on.tשo", is_pkg=True)
        # Find the same module from iter_modules()
        self.assertCountEqual(finder.iter_modules(""), [("on.tשo", True)])

    def test_find_spec_nested_abs_str(self):
        self.assert_find_spec_nested(os.path.join(sys.executable, "on"))

    def test_find_spec_nested_rel_str(self):
        exe = Path(sys.executable)
        with chdir(exe.parent):
            self.assert_find_spec_nested(str(Path("..", exe.parent.name, exe.name, "on")))

    def test_find_spec_nested_abs_pathlike(self):
        self.assert_find_spec_nested(Path(sys.executable, "on"))

    def test_find_spec_nested_rel_bytes(self):
        exe = Path(sys.executable)
        with chdir(exe.parent):
            self.assert_find_spec_nested(bytes(Path("..", exe.parent.name, exe.name, "on")))

    def assert_find_spec_top_level(self, path: Union[str, bytes, os.PathLike]) -> None:
        finder = self.finder(path, "")
        modules = [("a", True), ("one", True), ("on", True)]
        self.assertCountEqual(finder.iter_modules(""), modules)
        for name, is_pkg in modules:
            self.assert_spec(finder.find_spec(name), name, is_pkg)
        for name in "a.b", "a.b.c", "on.tשo", "on.tשo.۳":
            self.assertIsNone(finder.find_spec(name))

    def test_find_spec_top_level_abs_str(self):
        self.assert_find_spec_top_level(sys.executable)

    def test_find_spec_top_level_abs_bytes(self):
        self.assert_find_spec_top_level(os.fsencode(sys.executable))

    def test_find_spec_top_level_rel_str(self):
        exe = Path(sys.executable)
        with chdir(exe.parent):
            self.assert_find_spec_top_level(exe.name)

    def assert_unicode_path(self, path: Union[str, bytes, os.PathLike]) -> None:
        finder = self.finder(path, "on.tשo")
        self.assert_spec(finder.find_spec("on.tשo.۳"), "on.tשo.۳", is_pkg=False)
        self.assertCountEqual(finder.iter_modules(""), [("on.tשo.۳", False)])

    def test_unicode_path_abs_str(self):
        self.assert_unicode_path(os.path.join(sys.executable,"on", "tשo"))

    def test_unicode_path_abs_bytes(self):
        self.assert_unicode_path(os.fsencode(os.path.join(sys.executable,"on", "tשo")))

    def test_unicode_path_rel_pathlike(self):
        exe = Path(sys.executable)
        with chdir(exe.parent):
            self.assert_unicode_path(Path(exe.name, "on", "tשo"))

    def test_unicode_path_rel_bytes(self):
        exe = Path(sys.executable)
        with chdir(exe.parent):
            self.assert_unicode_path(bytes(Path(exe.name, "on", "tשo")))

    def test_empty_finder_abs_str(self):
        self.assertIsNone(OxidizedFinder().path_hook(sys.executable).find_spec("a"))

    def test_non_existent_pkg_abs_str(self):
        path = os.path.join(sys.executable, "foo", "bar")
        finder = self.finder(path, "foo.bar")
        self.assertIsNone(finder.find_spec("foo.bar.baz"))

    def test_non_existent_pkg_rel_str(self):
        exe = Path(sys.executable)
        with chdir(exe.parent):
            path = os.path.join("..", exe.parent.name, exe.name, "foo", "bar")
            finder = self.finder(path, "foo.bar")
            self.assertIsNone(finder.find_spec("foo.bar.baz"))

    def test_path_hook_installed(self):
        # PathFinder can only use it with sys.executable on sys.path
        with patch('sys.path', sys.path):
            sys.path = [p for p in sys.path if p != sys.executable]
            PathFinder.invalidate_caches()
            self.assertIsNone(PathFinder.find_spec("pwd"))

            sys.path.append(sys.executable)
            spec = PathFinder.find_spec("pwd")
        self.assert_spec(
            spec, "pwd", is_pkg=False, Loader=sys.__spec__.loader,
            origin="built-in")

    ############################################################################
    # Error Handling

    NOT_FOUND_ERR = "path .* does not begin in .*"

    def test_not_sys_executable_abs_str(self):
        self.assertFalse(
            sys.prefix.startswith(sys.executable), "bad assumption in test")
        finder = OxidizedFinder()
        with self.assertRaisesRegex(ImportError, self.NOT_FOUND_ERR) as cm:
             finder.path_hook(sys.prefix)
        self.assertEqual(cm.exception.path, sys.prefix)

    def test_not_sys_executable_rel_str(self):
        path = Path(Path(sys.executable).name, "a", "b")
        with tempfile.TemporaryDirectory(prefix="oxidized_importer-test-") as td:
            with chdir(td):
                with self.assertRaisesRegex(ImportError, self.NOT_FOUND_ERR) as cm:
                    self.finder(path, "a.b")
                self.assertEqual(cm.exception.path, path)

    def test_find_spec_no_path_arg(self):
        finder = self.finder(Path(sys.executable, "a"), "a")
        # finder.find_spec does not take a path arg
        self.assertRaisesRegex(
            TypeError, "takes at most 2 arguments", finder.find_spec, "a.b",
            None, None)
        self.assertRaisesRegex(
            TypeError, "'path' is an invalid keyword argument",
            finder.find_spec, "a.b", path=None)

    def test_path_bad_type(self):
        self.assertRaisesRegex(
            TypeError, "expected str, bytes or os.PathLike object, not int",
            OxidizedFinder().path_hook, 1)

    def test_bad_unicode_executable_path_ok(self):
        # A non-Unicode path doesn't cause a panic.
        exe = sys.executable.encode("utf-8", "surrogatepass")
        # "fo\ud800o" contains an unpaired surrogate.
        foo = "fo\ud800o".encode("utf-8", "surrogatepass")
        exe += foo
        with self.assertRaises(ImportError) as cm:
            OxidizedFinder().path_hook(exe)
        self.assertEqual(cm.exception.path, exe)

    @unittest.expectedFailure  # https://github.com/dgrunwald/rust-cpython/issues/246
    def test_bad_utf8_pkg_name_raises(self):
        exe = sys.executable.encode("utf-8", "surrogatepass")
        foo = "fo\ud800o".encode("utf-8", "surrogatepass")
        exe = os.path.join(exe, foo)
        with self.assertRaisesRegex(ImportError, "cannot decode") as cm:
            OxidizedFinder().path_hook(exe)
        self.assertIsInstance(cm.exception.__cause__, UnicodeDecodeError)
        self.assertIn(foo, cm.exception.__cause__.object)
        self.assertEqual(cm.exception.__cause__.encoding.lower(), "utf-8")


if __name__ == "__main__":
    unittest.main()
