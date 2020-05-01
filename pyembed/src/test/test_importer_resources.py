# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import sys
import unittest

from _pyoxidizer_importer import (
    OxidizedResource,
    PyOxidizerFinder as Finder,
)


class TestImporterConstruction(unittest.TestCase):
    def test_resources_builtins(self):
        f = Finder()
        resources = f.indexed_resources()
        self.assertIsInstance(resources, list)
        self.assertGreater(len(resources), 0)

        resources = sorted(resources, key=lambda x: x.name)

        resource = [x for x in resources if x.name == "_io"][0]

        self.assertIsInstance(resource, OxidizedResource)

        self.assertEqual(repr(resource), '<OxidizedResource name="_io">')

        self.assertIsInstance(resource.name, str)
        self.assertEqual(resource.name, "_io")

        self.assertFalse(resource.is_package)
        self.assertFalse(resource.is_namespace_package)
        self.assertIsNone(resource.in_memory_source)
        self.assertIsNone(resource.in_memory_bytecode)
        self.assertIsNone(resource.in_memory_bytecode_opt1)
        self.assertIsNone(resource.in_memory_bytecode_opt2)
        self.assertIsNone(resource.in_memory_extension_module_shared_library)
        self.assertIsNone(resource.in_memory_package_resources)
        self.assertIsNone(resource.in_memory_distribution_resources)
        self.assertIsNone(resource.in_memory_shared_library)
        self.assertIsNone(resource.shared_library_dependency_names)


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
