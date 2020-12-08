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

    def test_iter_modules_path(self):
        self._make_package("a.b.c")
        self._make_package("one.two.three")
        self._make_package("on.tשo.۳")
        self._make_package("on.two")
        path = pathlib.Path(sys.executable, "on")
        finder = self._finder_from_td()
        path_entry_finder = finder.path_hook(path)
        _PathEntryFinder = type(path_entry_finder)
        modules = path_entry_finder.iter_modules()
        self.assertCountEqual(modules, [("on.two", True), ("on.tשo", True)])

        sys.meta_path = [finder]
        import on
        self.assertEqual(on.__path__, [str(path)])
        sys.path = [sys.executable]
        with patch.object(sys, "path_hooks", [finder.path_hook]):
            res = list(pkgutil.iter_modules([path]))
            self.assertCountEqual([(mi.name, mi.ispkg) for mi in res], modules)
            for mi in res:
                self.assertIsInstance(mi, pkgutil.ModuleInfo)
                self.assertIsInstance(mi.module_finder, _PathEntryFinder)


if __name__ == "__main__":
    unittest.main()
