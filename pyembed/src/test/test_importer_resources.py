# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import importlib.machinery
import marshal
import pathlib
import sys
import unittest

from oxidized_importer import (
    OxidizedResource,
    OxidizedFinder,
)


class TestImporterResources(unittest.TestCase):
    def test_resources_builtins(self):
        f = OxidizedFinder()
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
        f = OxidizedFinder()
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

    def test_resource_in_memory_package_resources(self):
        resource = OxidizedResource()

        resource.in_memory_package_resources = {}
        self.assertEqual(resource.in_memory_package_resources, {})

        resource.in_memory_package_resources = None
        self.assertIsNone(resource.in_memory_package_resources)

        resource.in_memory_package_resources = {"foo": b"foo value"}
        self.assertEqual(resource.in_memory_package_resources, {"foo": b"foo value"})

        # Updating the dict does *not* work.
        resource.in_memory_package_resources["foo"] = "ignored"
        resource.in_memory_package_resources["ignored"] = None
        self.assertEqual(resource.in_memory_package_resources, {"foo": b"foo value"})

        with self.assertRaises(TypeError):
            del resource.in_memory_package_resources

        with self.assertRaises(TypeError):
            resource.in_memory_package_resources = True

        with self.assertRaises(TypeError):
            resource.in_memory_package_resources = []

        with self.assertRaises(TypeError):
            resource.in_memory_package_resources = {b"foo": b"bar"}

        with self.assertRaises(TypeError):
            resource.in_memory_package_resources = {"foo": None}

    def test_in_memory_distribution_resources(self):
        resource = OxidizedResource()

        resource.in_memory_distribution_resources = {}
        self.assertEqual(resource.in_memory_distribution_resources, {})

        resource.in_memory_distribution_resources = None
        self.assertIsNone(resource.in_memory_distribution_resources)

        resource.in_memory_distribution_resources = {"foo": b"foo value"}
        self.assertEqual(
            resource.in_memory_distribution_resources, {"foo": b"foo value"}
        )

        # Updating the dict does *not* work.
        resource.in_memory_distribution_resources["foo"] = "ignored"
        resource.in_memory_distribution_resources["ignored"] = None
        self.assertEqual(
            resource.in_memory_distribution_resources, {"foo": b"foo value"}
        )

        with self.assertRaises(TypeError):
            del resource.in_memory_distribution_resources

        with self.assertRaises(TypeError):
            resource.in_memory_distribution_resources = True

        with self.assertRaises(TypeError):
            resource.in_memory_distribution_resources = []

        with self.assertRaises(TypeError):
            resource.in_memory_distribution_resources = {b"foo": b"bar"}

        with self.assertRaises(TypeError):
            resource.in_memory_distribution_resources = {"foo": None}

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

    def test_resource_shared_library_dependency_names(self):
        resource = OxidizedResource()

        resource.shared_library_dependency_names = []
        self.assertEqual(resource.shared_library_dependency_names, [])

        resource.shared_library_dependency_names = None
        self.assertIsNone(resource.shared_library_dependency_names)

        resource.shared_library_dependency_names = ["foo"]
        self.assertEqual(resource.shared_library_dependency_names, ["foo"])

        # List mutation is not reflected in original object.
        resource.shared_library_dependency_names[:] = []
        resource.shared_library_dependency_names.append("bar")
        self.assertEqual(resource.shared_library_dependency_names, ["foo"])

        with self.assertRaises(TypeError):
            del resource.shared_library_dependency_names

        with self.assertRaises(TypeError):
            resource.shared_library_dependency_names = True

        with self.assertRaises(TypeError):
            resource.shared_library_dependency_names = [b"foo"]

    def test_resource_relative_path_module_source(self):
        resource = OxidizedResource()

        resource.relative_path_module_source = "lib/foo.py"
        self.assertEqual(
            resource.relative_path_module_source, pathlib.Path("lib/foo.py")
        )

        resource.relative_path_module_source = pathlib.Path("bar.py")
        self.assertEqual(resource.relative_path_module_source, pathlib.Path("bar.py"))

        resource.relative_path_module_source = b"foo.py"
        self.assertEqual(resource.relative_path_module_source, pathlib.Path("foo.py"))

        resource.relative_path_module_source = None
        self.assertIsNone(resource.relative_path_module_source)

        with self.assertRaises(TypeError):
            del resource.relative_path_module_source

        with self.assertRaises(TypeError):
            resource.relative_path_module_source = True

    def test_resource_relative_path_module_bytecode(self):
        resource = OxidizedResource()

        resource.relative_path_module_bytecode = "lib/foo.pyc"
        self.assertEqual(
            resource.relative_path_module_bytecode, pathlib.Path("lib/foo.pyc")
        )

        resource.relative_path_module_bytecode = pathlib.Path("bar.pyc")
        self.assertEqual(
            resource.relative_path_module_bytecode, pathlib.Path("bar.pyc")
        )

        resource.relative_path_module_bytecode = b"foo.pyc"
        self.assertEqual(
            resource.relative_path_module_bytecode, pathlib.Path("foo.pyc")
        )

        resource.relative_path_module_bytecode = None
        self.assertIsNone(resource.relative_path_module_bytecode)

        with self.assertRaises(TypeError):
            del resource.relative_path_module_bytecode

        with self.assertRaises(TypeError):
            resource.relative_path_module_bytecode = True

    def test_resource_relative_path_module_bytecode_opt1(self):
        resource = OxidizedResource()

        resource.relative_path_module_bytecode_opt1 = "lib/foo.pyc"
        self.assertEqual(
            resource.relative_path_module_bytecode_opt1, pathlib.Path("lib/foo.pyc")
        )

        resource.relative_path_module_bytecode_opt1 = pathlib.Path("bar.pyc")
        self.assertEqual(
            resource.relative_path_module_bytecode_opt1, pathlib.Path("bar.pyc")
        )

        resource.relative_path_module_bytecode_opt1 = b"foo.pyc"
        self.assertEqual(
            resource.relative_path_module_bytecode_opt1, pathlib.Path("foo.pyc")
        )

        resource.relative_path_module_bytecode_opt1 = None
        self.assertIsNone(resource.relative_path_module_bytecode_opt1)

        with self.assertRaises(TypeError):
            del resource.relative_path_module_bytecode_opt1

        with self.assertRaises(TypeError):
            resource.relative_path_module_bytecode_opt1 = True

    def test_resource_relative_path_module_bytecode_opt2(self):
        resource = OxidizedResource()

        resource.relative_path_module_bytecode_opt2 = "lib/foo.pyc"
        self.assertEqual(
            resource.relative_path_module_bytecode_opt2, pathlib.Path("lib/foo.pyc")
        )

        resource.relative_path_module_bytecode_opt2 = pathlib.Path("bar.pyc")
        self.assertEqual(
            resource.relative_path_module_bytecode_opt2, pathlib.Path("bar.pyc")
        )

        resource.relative_path_module_bytecode_opt2 = b"foo.pyc"
        self.assertEqual(
            resource.relative_path_module_bytecode_opt2, pathlib.Path("foo.pyc")
        )

        resource.relative_path_module_bytecode_opt2 = None
        self.assertIsNone(resource.relative_path_module_bytecode_opt2)

        with self.assertRaises(TypeError):
            del resource.relative_path_module_bytecode_opt2

        with self.assertRaises(TypeError):
            resource.relative_path_module_bytecode_opt2 = True

    def test_relative_path_extension_module_shared_library(self):
        resource = OxidizedResource()

        resource.relative_path_extension_module_shared_library = "lib/foo.so"
        self.assertEqual(
            resource.relative_path_extension_module_shared_library,
            pathlib.Path("lib/foo.so"),
        )

        resource.relative_path_extension_module_shared_library = pathlib.Path("bar.so")
        self.assertEqual(
            resource.relative_path_extension_module_shared_library,
            pathlib.Path("bar.so"),
        )

        resource.relative_path_extension_module_shared_library = b"foo.so"
        self.assertEqual(
            resource.relative_path_extension_module_shared_library,
            pathlib.Path("foo.so"),
        )

        resource.relative_path_extension_module_shared_library = None
        self.assertIsNone(resource.relative_path_extension_module_shared_library)

        with self.assertRaises(TypeError):
            del resource.relative_path_extension_module_shared_library

        with self.assertRaises(TypeError):
            resource.relative_path_extension_module_shared_library = True

    def test_relative_path_package_resources(self):
        resource = OxidizedResource()

        resource.relative_path_package_resources = {}
        self.assertEqual(resource.relative_path_package_resources, {})

        resource.relative_path_package_resources = None
        self.assertIsNone(resource.relative_path_package_resources)

        resource.relative_path_package_resources = {"foo": "resource.txt"}
        self.assertEqual(
            resource.relative_path_package_resources,
            {"foo": pathlib.Path("resource.txt")},
        )

        resource.relative_path_package_resources = {
            "resource.txt": pathlib.Path("path/to/resource")
        }
        self.assertEqual(
            resource.relative_path_package_resources,
            {"resource.txt": pathlib.Path("path/to/resource")},
        )

        # Updating the dict does *not* work.
        resource.relative_path_package_resources["foo"] = "ignored"
        resource.relative_path_package_resources["ignored"] = None
        self.assertEqual(
            resource.relative_path_package_resources,
            {"resource.txt": pathlib.Path("path/to/resource")},
        )

        with self.assertRaises(TypeError):
            del resource.relative_path_package_resources

        with self.assertRaises(TypeError):
            resource.relative_path_package_resources = True

        with self.assertRaises(TypeError):
            resource.relative_path_package_resources = []

        with self.assertRaises(TypeError):
            resource.relative_path_package_resources = {b"foo": b"bar"}

        with self.assertRaises(TypeError):
            resource.relative_path_package_resources = {"foo": None}

    def test_relative_path_distribution_resources(self):
        resource = OxidizedResource()

        resource.relative_path_distribution_resources = {}
        self.assertEqual(resource.relative_path_distribution_resources, {})

        resource.relative_path_distribution_resources = None
        self.assertIsNone(resource.relative_path_distribution_resources)

        resource.relative_path_distribution_resources = {"foo": "resource.txt"}
        self.assertEqual(
            resource.relative_path_distribution_resources,
            {"foo": pathlib.Path("resource.txt")},
        )

        resource.relative_path_distribution_resources = {
            "resource.txt": pathlib.Path("path/to/resource")
        }
        self.assertEqual(
            resource.relative_path_distribution_resources,
            {"resource.txt": pathlib.Path("path/to/resource")},
        )

        # Updating the dict does *not* work.
        resource.relative_path_distribution_resources["foo"] = "ignored"
        resource.relative_path_distribution_resources["ignored"] = None
        self.assertEqual(
            resource.relative_path_distribution_resources,
            {"resource.txt": pathlib.Path("path/to/resource")},
        )

        with self.assertRaises(TypeError):
            del resource.relative_path_distribution_resources

        with self.assertRaises(TypeError):
            resource.relative_path_distribution_resources = True

        with self.assertRaises(TypeError):
            resource.relative_path_distribution_resources = []

        with self.assertRaises(TypeError):
            resource.relative_path_distribution_resources = {b"foo": b"bar"}

        with self.assertRaises(TypeError):
            resource.relative_path_distribution_resources = {"foo": None}

    def test_add_resource_bad_type(self):
        f = OxidizedFinder()

        with self.assertRaises(TypeError):
            f.add_resource(None)

    def test_add_resource_module(self):
        f = OxidizedFinder()
        resource = OxidizedResource()
        resource.name = "my_module"
        resource.flavor = "module"

        source = b"print('hello from my_module')"
        code = compile(source, "my_module.py", "exec")
        bytecode = marshal.dumps(code)

        resource.in_memory_source = source
        resource.in_memory_bytecode = bytecode

        f.add_resource(resource)

        resources = [r for r in f.indexed_resources() if r.name == "my_module"]
        self.assertEqual(len(resources), 1)

        spec = f.find_spec("my_module", None)
        self.assertIsInstance(spec, importlib.machinery.ModuleSpec)
        self.assertEqual(spec.name, "my_module")
        self.assertIsNone(spec.loader_state)
        self.assertIsNone(spec.submodule_search_locations)

        self.assertEqual(f.get_source("my_module"), source.decode("utf-8"))
        self.assertEqual(f.get_code("my_module"), code)

    def test_add_resources(self):
        f = OxidizedFinder()
        a = OxidizedResource()
        a.name = "foo_a"
        a.flavor = "module"

        b = OxidizedResource()
        b.name = "foo_b"
        b.flavor = "module"

        f.add_resources([a, b])

        resources = [r for r in f.indexed_resources() if r.name in ("foo_a", "foo_b")]
        self.assertEqual(len(resources), 2)

    def test_serialize_simple(self):
        f = OxidizedFinder()

        m = OxidizedResource()
        m.name = "my_module"
        m.flavor = "module"
        m.in_memory_source = b"import io"
        f.add_resource(m)

        m = OxidizedResource()
        m.name = "module_b"
        m.flavor = "module"
        m.in_memory_bytecode = b"dummy bytecode"
        f.add_resource(m)

        serialized = f.serialize_indexed_resources()
        self.assertIsInstance(serialized, bytes)

        f2 = OxidizedFinder(resources_data=serialized)

        modules = {r.name: r for r in f2.indexed_resources() if r.flavor == "module"}
        self.assertEqual(len(modules), 2)

        self.assertIn("my_module", modules)
        self.assertIn("module_b", modules)

        self.assertEqual(modules["my_module"].in_memory_source, b"import io")
        self.assertEqual(modules["module_b"].in_memory_bytecode, b"dummy bytecode")


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
