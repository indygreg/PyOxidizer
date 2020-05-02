# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import importlib.machinery
import sys
import unittest


class TestImporterBuiltins(unittest.TestCase):
    def get_importer(self):
        self.assertEqual(len(sys.meta_path), 2)

        importer = sys.meta_path[0]
        self.assertEqual(importer.__class__.__name__, "OxidizedFinder")

        return importer

    def test_find_spec(self):
        importer = self.get_importer()

        spec = importer.find_spec("_io", None)
        self.assertIsInstance(spec, importlib.machinery.ModuleSpec)
        self.assertEqual(spec.name, "_io")
        self.assertIn("BuiltinImporter", str(spec.loader))
        self.assertEqual(spec.origin, "built-in")
        self.assertIsNone(spec.loader_state)
        self.assertIsNone(spec.submodule_search_locations)

    def test_find_module(self):
        importer = self.get_importer()

        loader = importer.find_module("_io", None)
        self.assertIn("BuiltinImporter", str(loader))

    def test_get_code(self):
        importer = self.get_importer()
        self.assertIsNone(importer.get_code("_io"))

    def test_get_source(self):
        importer = self.get_importer()
        self.assertIsNone(importer.get_source("_io"))

    def test_get_filename(self):
        importer = self.get_importer()

        with self.assertRaises(ImportError):
            importer.get_filename("_io")


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
