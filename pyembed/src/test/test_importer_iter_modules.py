# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import os
import pathlib
import pkgutil
import sys
import tempfile
import unittest
from unittest.mock import patch

from oxidized_importer import (
    OxidizedFinder,
    OxidizedResourceCollector,
    find_resources_in_path,
)


class TestImporterIterModules(unittest.TestCase):
    def setUp(self):
        self.raw_temp_dir = tempfile.TemporaryDirectory(
            prefix="oxidized_importer-test-"
        )
        self.td = pathlib.Path(self.raw_temp_dir.name)

        self.old_finders = list(sys.meta_path)
        self.old_path = list(sys.path)

    def tearDown(self):
        sys.path[:] = self.old_path
        sys.meta_path[:] = self.old_finders

        self.raw_temp_dir.cleanup()
        del self.raw_temp_dir
        del self.td

    def _make_package(self, name):
        package_path = self.td

        for part in name.split("."):
            package_path = package_path / part
            package_path.mkdir(exist_ok=True)

            with (package_path / "__init__.py").open("wb"):
                pass

        return package_path

    def _finder_from_td(self):
        collector = OxidizedResourceCollector(allowed_locations=["in-memory"])
        for r in find_resources_in_path(self.td):
            collector.add_in_memory(r)

        f = OxidizedFinder()
        f.add_resources(
            collector.oxidize(python_exe=os.environ.get("PYTHON_SYS_EXECUTABLE"))[0]
        )

        return f

    def test_iter_modules_empty(self):
        f = OxidizedFinder()
        self.assertIsInstance(f.iter_modules(), list)
        self.assertEqual(f.iter_modules(), [])

        sys.meta_path = [f]
        sys.path = []
        self.assertEqual(len(list(pkgutil.iter_modules())), 0)

    def test_iter_modules_single(self):
        p = self._make_package("my_package")
        with (p / "__init__.py").open("wb") as fh:
            fh.write(b"import io\n")

        f = self._finder_from_td()

        modules = f.iter_modules()
        self.assertEqual(len(modules), 1)
        self.assertEqual(modules[0], ("my_package", True))

        sys.meta_path = [f]
        sys.path = []
        res = list(pkgutil.iter_modules())
        self.assertEqual(len(res), 1)
        self.assertIsInstance(res[0], pkgutil.ModuleInfo)
        self.assertEqual(res[0].module_finder.__class__.__name__, "OxidizedFinder")
        self.assertEqual(res[0].name, "my_package")
        self.assertTrue(res[0].ispkg)

    def test_iter_modules_prefix(self):
        p = self._make_package("my_package")
        with (p / "__init__.py").open("wb") as fh:
            fh.write(b"import io\n")

        f = self._finder_from_td()

        modules = f.iter_modules("foo")
        self.assertEqual(len(modules), 1)
        self.assertEqual(modules[0], ("foomy_package", True))

        sys.meta_path = [f]
        sys.path = []
        res = list(pkgutil.iter_modules(prefix="foo"))
        self.assertEqual(len(res), 1)
        self.assertIsInstance(res[0], pkgutil.ModuleInfo)
        self.assertEqual(res[0].module_finder.__class__.__name__, "OxidizedFinder")
        self.assertEqual(res[0].name, "foomy_package")
        self.assertTrue(res[0].ispkg)

    def test_iter_modules_nested(self):
        self._make_package("a.b")
        (self.td / "one.py").touch()

        f = self._finder_from_td()

        expected = [("a", True), ("one", False)]
        self.assertCountEqual(f.iter_modules(), expected)

        sys.meta_path = [f]
        sys.path = []
        res = list(pkgutil.iter_modules())
        self.assertCountEqual([(mi.name, mi.ispkg) for mi in res], expected)
        for mi in res:
            self.assertIs(mi.module_finder, f)

    def test_iter_modules_path(self):
        self._make_package("a.b.c")
        self._make_package("one.two.three")
        self._make_package("on.tשo.۳")
        self._make_package("on.two")

        f = self._finder_from_td()

        name = "on"
        path = pathlib.Path(sys.executable, name)
        path_entry_finder = f.path_hook(path)

        with self.subTest(prefix="", module_iterator="_PathEntryFinder"):
            unprefixed = path_entry_finder.iter_modules()
            self.assertCountEqual(unprefixed, [("two", True), ("tשo", True)])

        prefix = name + "."
        with self.subTest(prefix=name + ".", module_iterator="_PathEntryFinder"):
            prefixed = path_entry_finder.iter_modules(prefix=prefix)
            self.assertCountEqual(prefixed, [("on.two", True), ("on.tשo", True)])

        def assert_iter_modules(prefix: str, expected, *args):
            with self.subTest(prefix=prefix, module_iterator="pkgutil"):
                res = list(pkgutil.iter_modules(*args))
                self.assertCountEqual([(mi.name, mi.ispkg) for mi in res], expected)
                for mi in res:
                    self.assertIsInstance(mi.module_finder, type(path_entry_finder))
                    self.assertEqual(
                        mi.module_finder._package, path_entry_finder._package)

        sys.path = [sys.executable]
        sys.meta_path = [f]
        with patch.dict(sys.modules):
            import on
            self.assertEqual(on.__name__, name)
            self.assertEqual(on.__path__, [str(path)])
            with patch.object(sys, "path_hooks", [f.path_hook]):
                assert_iter_modules("", unprefixed, on.__path__)
                prefix = on.__name__ + "."
                assert_iter_modules(prefix, prefixed, on.__path__, prefix)


if __name__ == "__main__":
    unittest.main()
