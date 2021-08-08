# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
from __future__ import annotations

from contextlib import contextmanager
from importlib.machinery import PathFinder
import marshal
import os
from pathlib import Path
import sys
from typing import Iterable, Optional, Tuple, Union, TYPE_CHECKING
import unittest
from unittest.mock import patch

from oxidized_importer import OxidizedFinder, OxidizedResource, OxidizedPathEntryFinder

if TYPE_CHECKING:
    import importlib.abc
    from importlib.machinery import ModuleSpec


PathLike = Union[str, bytes, os.PathLike]

PATH_HOOK_BASE_STR = OxidizedFinder().path_hook_base_str


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
        resource.in_memory_bytecode = marshal.dumps(
            compile(source, module_name, "exec")
        )
        mpf.add_resource(resource)
    return mpf


@contextmanager
def chdir(dir: os.PathLike) -> Iterable[Path]:
    "Change the current directory to ``dir``, yielding the previous one."
    old_cwd = Path.cwd()
    try:
        os.chdir(dir)
        yield old_cwd
    finally:
        os.chdir(old_cwd)


class TestImporterPathEntryFinder(unittest.TestCase):
    def finder(self, path: os.PathLike, package: str) -> OxidizedPathEntryFinder:
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
        self.assertIsInstance(pef, OxidizedPathEntryFinder)
        self.assertEqual(pef._package, package)
        self.assertRaises(AttributeError, setattr, pef, "_package", package)
        return pef

    def assert_spec(
        self,
        spec: ModuleSpec,
        name: str,
        is_pkg: bool,
        Loader: importlib.abc.Loader = OxidizedFinder,
        origin: Optional[str] = None,
    ) -> None:
        self.assertIsNotNone(spec, name)
        self.assertEqual(spec.name, name, spec)
        self.assertTrue(
            isinstance(spec.loader, Loader) or issubclass(spec.loader, Loader), spec
        )
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

    def assert_find_spec_nested(self, path: os.PathLike) -> None:
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
        # Find the same module from iter_modules(), without and with a prefix
        self.assertCountEqual(finder.iter_modules(), [("tשo", True)])
        self.assertCountEqual(finder.iter_modules("on."), [("on.tשo", True)])

    def test_bytes_path_rejected(self):
        f = OxidizedFinder()

        with self.assertRaisesRegex(
            ImportError, "error running OxidizedFinder.path_hook"
        ) as e:
            f.path_hook(b"foo")

        self.assertIsInstance(e.exception.__cause__, TypeError)

    def test_path_pathlike_rejected(self):
        f = OxidizedFinder()

        with self.assertRaisesRegex(
            ImportError, "error running OxidizedFinder.path_hook"
        ) as e:
            f.path_hook(Path("foo"))

        self.assertIsInstance(e.exception.__cause__, TypeError)

    def test_path_hook_base_str_ok(self):
        f = OxidizedFinder()
        self.assertIsInstance(f.path_hook(PATH_HOOK_BASE_STR), OxidizedPathEntryFinder)

    def test_path_hook_base_str_parent_rejected(self):
        f = OxidizedFinder()

        with self.assertRaises(ImportError):
            f.path_hook(os.path.dirname(PATH_HOOK_BASE_STR))

    def test_path_hook_empty_rejected(self):
        f = OxidizedFinder()

        with self.assertRaises(ImportError):
            f.path_hook("")

    def test_dots_rejected(self):
        f = OxidizedFinder()

        suffixes = (
            r".",
            r"..",
            r".foo",
            r"foo.",
            r"foo..bar",
            r"../../etc/passwd",
            r"..\..\etc\passwd",
        )

        for suffix in suffixes:
            with self.assertRaises(ImportError):
                f.path_hook(PATH_HOOK_BASE_STR + "/" + suffix)

            with self.assertRaises(ImportError):
                f.path_hook(PATH_HOOK_BASE_STR + "\\" + suffix)

    def test_bad_directory_separators(self):
        suffixes = (
            r"//foo",
            r"/foo//bar",
            r"/foo\\bar",
            r"\\foo",
            r"/foo//",
            r"/foo\\",
            r"/\foo",
        )

        for suffix in suffixes:
            f = OxidizedFinder()

            with self.assertRaises(ImportError):
                f.path_hook(PATH_HOOK_BASE_STR + suffix)

    def test_package_resolution(self):
        mapping = (
            (r"/foo", "foo"),
            (r"/foo/bar", "foo.bar"),
            (r"\foo", "foo"),
            (r"\foo\bar", "foo.bar"),
            (r"/foo\bar", "foo.bar"),
            (r"\foo/bar", "foo.bar"),
            (r"/foo.bar", "foo.bar"),
            (r"\foo.bar", "foo.bar"),
            (r"/foo.bar/baz", "foo.bar.baz"),
            (r"/tשo", "tשo"),
            (r"\tשo", "tשo"),
        )

        for (suffix, package) in mapping:
            f = OxidizedFinder()

            pef = f.path_hook(PATH_HOOK_BASE_STR + suffix)
            self.assertEqual(pef._package, package)

    def test_find_spec_subdir(self):
        self.assert_find_spec_nested(os.path.join(PATH_HOOK_BASE_STR, "on"))

    def assert_find_spec_top_level(self, path: os.PathLike) -> None:
        finder = self.finder(path, None)
        modules = [("a", True), ("one", True), ("on", True)]
        self.assertCountEqual(finder.iter_modules(), modules)

        for name, is_pkg in modules:
            self.assert_spec(finder.find_spec(name), name, is_pkg)
        for name in "a.b", "a.b.c", "on.tשo", "on.tשo.۳":
            self.assertIsNone(finder.find_spec(name))

    def test_find_spec_top_level(self):
        self.assert_find_spec_top_level(PATH_HOOK_BASE_STR)

    def assert_unicode_path(self, path: os.PathLike) -> None:
        finder = self.finder(path, "on.tשo")
        self.assert_spec(finder.find_spec("on.tשo.۳"), "on.tשo.۳", is_pkg=False)
        self.assertCountEqual(finder.iter_modules(), [("۳", False)])

    def test_unicode_path_subdir(self):
        self.assert_unicode_path(os.path.join(PATH_HOOK_BASE_STR, "on", "tשo"))

    def test_empty_finder_top_level(self):
        self.assertIsNone(OxidizedFinder().path_hook(PATH_HOOK_BASE_STR).find_spec("a"))

    def test_non_existent_pkg(self):
        path = os.path.join(PATH_HOOK_BASE_STR, "foo", "bar")
        finder = self.finder(path, "foo.bar")
        self.assertIsNone(finder.find_spec("foo.bar.baz"))

    def test_path_hook_installed(self):
        if not PathFinder.find_spec("pwd"):
            raise unittest.SkipTest("PathFinder failed to import pwd")

        # PathFinder can only use it with CURRENT_EXT on sys.path
        with patch("sys.path", sys.path):
            sys.path = [p for p in sys.path if p != PATH_HOOK_BASE_STR]
            PathFinder.invalidate_caches()
            self.assertIsNone(PathFinder.find_spec("pwd"))

            sys.path.append(PATH_HOOK_BASE_STR)
            spec = PathFinder.find_spec("pwd")
        self.assert_spec(
            spec, "pwd", is_pkg=False, Loader=sys.__spec__.loader, origin="built-in"
        )

    ############################################################################
    # Error Handling

    NOT_FOUND_ERR = "path .* does not begin in .*"

    def test_find_spec_no_path_arg(self):
        finder = self.finder(os.path.join(PATH_HOOK_BASE_STR, "a"), "a")
        # finder.find_spec does not take a path arg
        self.assertRaisesRegex(
            TypeError,
            r"OxidizedPathEntryFinder\.find_spec\(\) takes from 1 to 2 positional arguments but 3 were given",
            finder.find_spec,
            "a.b",
            None,
            None,
        )
        self.assertRaisesRegex(
            TypeError,
            r"OxidizedPathEntryFinder\.find_spec\(\) got an unexpected keyword argument 'path'",
            finder.find_spec,
            "a.b",
            path=None,
        )


if __name__ == "__main__":
    unittest.main()
