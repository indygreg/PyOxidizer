# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

from importlib.machinery import ModuleSpec, PathFinder
import marshal
import os.path
import sys
import unittest

from oxidized_importer import OxidizedFinder, OxidizedResource


class TestImporterPathEntryFinder(unittest.TestCase):

    def add_packages(self, finder: OxidizedFinder) -> OxidizedFinder:
        """Add the following package hierarchy to ``finder``:

        - a imports a.b imports a.b.c
        - one imports three from .two; one.two imports one.two.three
        """

        def add(module_name: str, source: str, is_pkg: bool) -> None:
            """See example in OxidizedFinder.add_resource."""
            resource = OxidizedResource()
            resource.name = module_name
            resource.is_package = is_pkg
            resource.is_module = True
            resource.in_memory_bytecode = marshal.dumps(compile(
                source, module_name, "exec"))
            resource.in_memory_source = source.encode("utf-8")
            finder.add_resource(resource)

        add("a", "import a.b", True)
        add("a.b", "import a.b.c", True)
        add("a.b.c", "pass", False)

        add("one", "from .two import three", True)
        add("one.two", "import one.two.three", True)
        add("one.two.three", "pass", False)

        return finder

    def test_path_default(self):
        finder = sys.meta_path[0]
        self.assertIsInstance(finder, OxidizedFinder)
        self.assertIsNone(finder.path)

    def assert_oxide_spec_non_pkg(self, spec: ModuleSpec, name: str, is_pkg: bool) -> None:
        self.assertIsNotNone(spec, name)
        self.assertEqual(spec.name, name, spec)
        self.assertIsInstance(spec.loader, OxidizedFinder, spec)
        self.assertIsNone(spec.origin, spec)
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

    def test_find_spec(self):
        path = os.path.join(sys.executable, "a")
        finder = self.add_packages(OxidizedFinder(path=path))
        self.assertEqual(finder.path, path)

        # Return None for modules outside the search path
        self.assertIsNone(finder.find_spec("one.two"))

        # Return a correct ModuleSpec for modules in the search path
        spec = finder.find_spec("a.b")
        self.assert_oxide_spec_non_pkg(spec, "a.b", is_pkg=True)

        # Since finder.path is not None, finder.find_spec does not take a path arg
        self.assertRaisesRegex(
            ValueError, "does not take a path argument", finder.find_spec,
            "a.b", spec.submodule_search_locations, None)

    def test_path_hook_installed(self):
        PathFinder.invalidate_caches()
        spec = PathFinder.find_spec("pwd")
        self.assert_oxide_spec_non_pkg(spec, "pwd", is_pkg=False)

    def test_path_does_not_start_with_sys_executable(self):
        self.assertFalse(sys.prefix.startswith(sys.executable))
        finder = self.add_packages(OxidizedFinder(path=sys.prefix))
        self.assertEqual(finder.path, sys.prefix)
        for name in "a", "a.b", "a.b.c", "one", "one.two", "one.two.three":
            self.assertIsNone(finder.find_spec(name))

    def test_path_equals_sys_executable(self):
        finder = self.add_packages(OxidizedFinder(path=sys.executable))
        self.assertEqual(finder.path, sys.executable)
        for name in "a", "a.b", "a.b.c", "one", "one.two", "one.two.three":
            self.assert_oxide_spec_non_pkg(
                finder.find_spec(name), name,
                is_pkg=sum(c == "." for c in name) < 2)

    def test_path_relative(self):
        finder = self.add_packages(OxidizedFinder(path="one"))
        self.assertEqual(finder.path, "one")
        self.assert_oxide_spec_non_pkg(
            finder.find_spec("one.two"), "one.two", is_pkg=True)

    def test_iter_modules(self):
        path = os.path.join(sys.executable, "one")
        finder = self.add_packages(OxidizedFinder(path=path))
        self.assertCountEqual(
            finder.iter_modules(""),
            {("one", True), ("one.two", True), ("one.two.three", False)})

if __name__ == "__main__":
    unittest.main()
