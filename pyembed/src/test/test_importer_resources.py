# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import sys
import unittest

from _pyoxidizer_importer import (
    OxidizedResource,
    PyOxidizerFinder as Finder,
)


class TestImporterResources(unittest.TestCase):
    def test_resources_builtins(self):
        f = Finder()
        resources = f.indexed_resources()
        self.assertIsInstance(resources, list)
        self.assertGreater(len(resources), 0)

        resources = sorted(resources, key=lambda x: x.name)

        resource = [x for x in resources if x.name == "_io"][0]

        self.assertIsInstance(resource, OxidizedResource)

        self.assertEqual(repr(resource), '<OxidizedResource name="_io">')

        self.assertIsInstance(resource.flavor, str)
        self.assertEqual(resource.flavor, "builtin")
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
        self.assertIsNone(resource.relative_path_module_source)
        self.assertIsNone(resource.relative_path_module_bytecode)
        self.assertIsNone(resource.relative_path_module_bytecode_opt1)
        self.assertIsNone(resource.relative_path_module_bytecode_opt2)
        self.assertIsNone(resource.relative_path_extension_module_shared_library)
        self.assertIsNone(resource.relative_path_package_resources)
        self.assertIsNone(resource.relative_path_distribution_resources)

    def test_resources_frozen(self):
        f = Finder()
        resources = f.indexed_resources()

        resource = [x for x in resources if x.name == "_frozen_importlib"][0]
        self.assertEqual(resource.flavor, "frozen")

    def test_resource_constructor(self):
        resource = OxidizedResource()
        self.assertIsInstance(resource, OxidizedResource)
        self.assertEqual(resource.flavor, "none")
        self.assertEqual(resource.name, "")

    def test_resource_set_name(self):
        resource = OxidizedResource()

        resource.name = "foobar"
        self.assertEqual(resource.name, "foobar")

        with self.assertRaises(TypeError):
            del resource.name

        with self.assertRaises(TypeError):
            resource.name = None

    def test_resource_set_flavor(self):
        resource = OxidizedResource()

        for flavor in (
            "module",
            "none",
            "builtin",
            "frozen",
            "extension",
            "shared_library",
        ):
            resource.flavor = flavor
            self.assertEqual(resource.flavor, flavor)

        with self.assertRaises(TypeError):
            del resource.flavor

        with self.assertRaises(TypeError):
            resource.flavor = None

        with self.assertRaisesRegex(ValueError, "unknown resource flavor"):
            resource.flavor = "foo"

    def test_resource_set_package(self):
        resource = OxidizedResource()

        resource.is_package = True
        self.assertTrue(resource.is_package)
        resource.is_package = False
        self.assertFalse(resource.is_package)

        with self.assertRaises(TypeError):
            del resource.is_package

        with self.assertRaises(TypeError):
            resource.is_package = "foo"

    def test_resource_set_namespace_package(self):
        resource = OxidizedResource()

        resource.is_namespace_package = True
        self.assertTrue(resource.is_namespace_package)
        resource.is_namespace_package = False
        self.assertFalse(resource.is_namespace_package)

        with self.assertRaises(TypeError):
            del resource.is_namespace_package

        with self.assertRaises(TypeError):
            resource.is_namespace_package = "foo"

    def test_resource_set_in_memory_source(self):
        resource = OxidizedResource()

        # bytes works.
        resource.in_memory_source = b"import os"
        self.assertEqual(resource.in_memory_source, b"import os")

        # memoryview works.
        resource.in_memory_source = memoryview(b"import io")
        self.assertEqual(resource.in_memory_source, b"import io")

        # bytearray works.
        resource.in_memory_source = bytearray(b"foo bar")
        self.assertEqual(resource.in_memory_source, b"foo bar")

        resource.in_memory_source = None
        self.assertIsNone(resource.in_memory_source)

        with self.assertRaises(TypeError):
            del resource.in_memory_source

        with self.assertRaises(TypeError):
            resource.in_memory_source = True

        with self.assertRaises(TypeError):
            resource.in_memory_source = "import foo"

    def test_resource_set_in_memory_bytecode(self):
        resource = OxidizedResource()

        resource.in_memory_bytecode = b"b0"
        resource.in_memory_bytecode_opt1 = b"b1"
        resource.in_memory_bytecode_opt2 = b"b2"

        self.assertEqual(resource.in_memory_bytecode, b"b0")
        self.assertEqual(resource.in_memory_bytecode_opt1, b"b1")
        self.assertEqual(resource.in_memory_bytecode_opt2, b"b2")

        resource.in_memory_bytecode = None
        self.assertIsNone(resource.in_memory_bytecode)
        resource.in_memory_bytecode_opt1 = None
        self.assertIsNone(resource.in_memory_bytecode_opt1)
        resource.in_memory_bytecode_opt2 = None
        self.assertIsNone(resource.in_memory_bytecode_opt2)

        with self.assertRaises(TypeError):
            del resource.in_memory_bytecode
        with self.assertRaises(TypeError):
            del resource.in_memory_bytecode_opt1
        with self.assertRaises(TypeError):
            del resource.in_memory_bytecode_opt2

        with self.assertRaises(TypeError):
            resource.in_memory_bytecode = True
        with self.assertRaises(TypeError):
            resource.in_memory_bytecode_opt1 = "foo"
        with self.assertRaises(TypeError):
            resource.in_memory_bytecode_opt2 = False

    def test_resource_in_memory_extension_module(self):
        resource = OxidizedResource()

        resource.in_memory_extension_module_shared_library = b"ELF"
        self.assertEqual(resource.in_memory_extension_module_shared_library, b"ELF")

        resource.in_memory_extension_module_shared_library = None
        self.assertIsNone(resource.in_memory_extension_module_shared_library)

        with self.assertRaises(TypeError):
            del resource.in_memory_extension_module_shared_library

        with self.assertRaises(TypeError):
            resource.in_memory_extension_module_shared_library = "ELF"

    def test_resource_in_memory_shared_library(self):
        resource = OxidizedResource()

        resource.in_memory_shared_library = b"ELF"
        self.assertEqual(resource.in_memory_shared_library, b"ELF")

        resource.in_memory_shared_library = None
        self.assertIsNone(resource.in_memory_shared_library)

        with self.assertRaises(TypeError):
            del resource.in_memory_shared_library

        with self.assertRaises(TypeError):
            resource.in_memory_shared_library = "ELF"


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
