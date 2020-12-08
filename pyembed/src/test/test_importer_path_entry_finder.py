# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

from importlib.machinery import ModuleSpec, PathFinder
import os.path
import sys
import unittest

from oxidized_importer import OxidizedFinder


class TestImporterPathEntryFinder(unittest.TestCase):

    def test_path_default(self):
        finder = sys.meta_path[0]
        self.assertIsInstance(finder, OxidizedFinder)
        self.assertIsNone(finder.path)

    def assert_oxide_spec_non_pkg(self, spec: ModuleSpec, name: str) -> None:
        self.assertEqual(spec.name, name, spec)
        self.assertIsInstance(spec.loader, OxidizedFinder, spec)
        self.assertIsNone(spec.origin, spec)
        self.assertIsNone(spec.submodule_search_locations, spec)
        self.assertIsNone(spec.cached, spec)
        self.assertEqual(spec.parent, name.rpartition(.)[2], spec)
        self.assertFalse(spec.has_location, spec)

    def test_find_spec(self):
        path = os.path.join(sys.executable, "encodings")
        finder = OxidizedFinder(path=path)
        self.assertEqual(finder.path, path)

        # Return None for modules outside the search path
        self.assertIsNone(finder.find_spec("importlib.resources"))

        # Return a correct ModuleSpec for modules in the search path
        self.assert_oxide_spec_non_pkg(
            finder.find_spec("encodings.idna"), "encodings.idna")

        # Since finder.path is not None, finder.find_spec does not take a path arg
        import encodings
        self.assertRaises(
            TypeError, finder.find_spec, "encodings.idna", encodings.__path__,
            None)

    def test_path_hook(self):
        hook = OxidizedFinder.path_hook()
        self.assertTrue(callable(hook), hook)
        path = os.path.join(sys.executable, "encodings")
        instance = hook(path)
        self.assertIsInstance(instance, OxidizedFinder)
        self.assertEqual(instance.path, path)

    def test_path_hook_installed(self):
        PathFinder.invalidate_caches()
        spec = PathFinder.find_spec("urllib.request")
        self.assert_oxide_spec_non_pkg(spec, "urllib.request")

    def test_path_does_not_start_with_sys_executable(self):
        self.assertFalse(sys.prefix.startswith(sys.executable))
        finder = OxidizedFinder(path=sys.prefix)
        self.assertEqual(finder.path, sys.prefix)
        for name in "os", "io", "importlib":
            self.assertIsNone(finder.find_spec(name))

    def test_path_equals_sys_executable(self):
        finder = OxidizedFinder(path=sys.executable)
        self.assertEqual(finder.path, sys.executable)
        for name in "os", "io", "importlib", "importlib.resources":
            self.assert_oxide_spec_non_pkg(finder.find_spec(name), name)

    def test_path_relative(self):
        finder = OxidizedFinder(path="importlib")
        self.assertEqual(finder.path, os.path.join(sys.executable, "importlib"))
        self.assert_oxide_spec_non_pkg(
            finder.find_spec("importlib.resources"), "importlib.resources")


if __name__ == "__main__":
    unittest.main()
