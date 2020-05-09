# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import importlib.machinery
import importlib.util
import marshal
import os
import pathlib
import sys
import tempfile
import unittest

from oxidized_importer import (
    OxidizedFinder,
    OxidizedResourceCollector,
    find_resources_in_path,
)


class TestImporterModuleLoading(unittest.TestCase):
    def setUp(self):
        self.raw_temp_dir = tempfile.TemporaryDirectory(
            prefix="oxidized_importer-test-"
        )
        self.td = pathlib.Path(self.raw_temp_dir.name)

    def tearDown(self):
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
        collector = OxidizedResourceCollector(policy="in-memory-only")
        for r in find_resources_in_path(self.td):
            collector.add_in_memory(r)

        f = OxidizedFinder()
        f.add_resources(collector.oxidize()[0])

        return f

    def test_find_spec_missing(self):
        f = OxidizedFinder()

        self.assertIsNone(f.find_spec("my_package", None))

    def test_source_package(self):
        p = self._make_package("my_package")

        with (p / "__init__.py").open("wb") as fh:
            fh.write(b"import io\n")

        f = self._finder_from_td()

        spec = f.find_spec("my_package", None)
        self.assertIsInstance(spec, importlib.machinery.ModuleSpec)
        self.assertEqual(spec.name, "my_package")
        self.assertIsInstance(spec.loader, OxidizedFinder)
        self.assertIsNone(spec.origin)
        self.assertIsNone(spec.loader_state)
        self.assertIsInstance(spec.submodule_search_locations, list)
        self.assertEqual(
            spec.submodule_search_locations, [os.path.join(sys.argv[0], "my_package")],
        )

        # Default module creation semantics for source modules.
        self.assertIsNone(f.create_module(spec))

        m = importlib.util.module_from_spec(spec)
        self.assertEqual(m.__name__, "my_package")
        self.assertIsInstance(m.__loader__, OxidizedFinder)
        self.assertEqual(m.__loader__, f)
        self.assertEqual(m.__package__, "my_package")
        self.assertEqual(m.__path__, [os.path.join(sys.argv[0], "my_package")])
        self.assertFalse(hasattr(m, "__file__"))
        self.assertFalse(hasattr(m, "__cached__"))

        self.assertIsNone(f.exec_module(m))

        self.assertEqual(f.get_source("my_package"), "import io\n")

        code = compile(f.get_source("my_package"), "my_package", "exec")
        self.assertEqual(f.get_code("my_package"), code)

        with self.assertRaises(ImportError):
            f.get_filename("my_package")

    def test_bytecode_package(self):
        p = self._make_package("my_package")

        (p / "__pycache__").mkdir()

        with (
            p / "__pycache__" / ("__init__.%s.pyc" % sys.implementation.cache_tag)
        ).open("wb") as fh:
            fh.write(b"0123456789abcdef")

            code = compile("import io", "my_package", "exec")
            fh.write(marshal.dumps(code))

        f = self._finder_from_td()

        spec = f.find_spec("my_package", None)
        self.assertIsInstance(spec, importlib.machinery.ModuleSpec)
        self.assertEqual(spec.name, "my_package")
        self.assertIsInstance(spec.loader, OxidizedFinder)
        self.assertIsNone(spec.origin)
        self.assertIsNone(spec.loader_state)
        self.assertIsInstance(spec.submodule_search_locations, list)
        self.assertEqual(
            spec.submodule_search_locations, [os.path.join(sys.argv[0], "my_package")],
        )

        # Default module creation semantics for bytecode modules.
        self.assertIsNone(f.create_module(spec))

        m = importlib.util.module_from_spec(spec)
        self.assertEqual(m.__name__, "my_package")
        self.assertIsInstance(m.__loader__, OxidizedFinder)
        self.assertEqual(m.__loader__, f)
        self.assertEqual(m.__package__, "my_package")
        self.assertEqual(m.__path__, [os.path.join(sys.argv[0], "my_package")])
        self.assertFalse(hasattr(m, "__file__"))
        self.assertFalse(hasattr(m, "__cached__"))

        self.assertIsNone(f.exec_module(m))

        self.assertEqual(f.get_source("my_package"), "")

        with self.assertRaises(ImportError):
            f.get_filename("my_package")


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
