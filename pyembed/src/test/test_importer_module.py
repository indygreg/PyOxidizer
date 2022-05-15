# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import unittest

SYMBOL_ATTRIBUTES = {
    "OxidizedDistribution": {
        "_normalized_name",
        "discover",
        "entry_points",
        "files",
        "from_name",
        "metadata",
        "name",
        "read_text",
        "requires",
        "version",
    },
    "OxidizedFinder": {
        "add_resource",
        "add_resources",
        "create_module",
        "exec_module",
        "find_distributions",
        "find_module",
        "find_spec",
        "get_code",
        "get_data",
        "get_filename",
        "get_resource_reader",
        "get_source",
        "index_bytes",
        "index_file_memory_mapped",
        "index_interpreter_builtins",
        "index_interpreter_builtin_extension_modules",
        "index_interpreter_frozen_modules",
        "indexed_resources",
        "invalidate_caches",
        "iter_modules",
        "multiprocessing_set_start_method",
        "origin",
        "path_hook",
        "path_hook_base_str",
        "pkg_resources_import_auto_register",
        "serialize_indexed_resources",
    },
    "OxidizedPathEntryFinder": {
        "_package",
        "find_spec",
        "invalidate_caches",
        "iter_modules",
    },
    "OxidizedPkgResourcesProvider": {
        "get_metadata",
        "get_metadata_lines",
        "get_resource_filename",
        "get_resource_string",
        "get_resource_stream",
        "has_metadata",
        "has_resource",
        "metadata_isdir",
        "metadata_listdir",
        "resource_isdir",
        "resource_listdir",
        "run_script",
    },
    "OxidizedResource": {
        "in_memory_bytecode_opt1",
        "in_memory_bytecode_opt2",
        "in_memory_bytecode",
        "in_memory_distribution_resources",
        "in_memory_extension_module_shared_library",
        "in_memory_package_resources",
        "in_memory_shared_library",
        "in_memory_source",
        "is_builtin_extension_module",
        "is_extension_module",
        "is_frozen_module",
        "is_module",
        "is_namespace_package",
        "is_package",
        "is_shared_library",
        "name",
        "relative_path_distribution_resources",
        "relative_path_extension_module_shared_library",
        "relative_path_module_bytecode_opt1",
        "relative_path_module_bytecode_opt2",
        "relative_path_module_bytecode",
        "relative_path_module_source",
        "relative_path_package_resources",
        "shared_library_dependency_names",
    },
    "OxidizedResourceCollector": {
        "add_filesystem_relative",
        "add_in_memory",
        "allowed_locations",
        "oxidize",
    },
    "OxidizedResourceReader": {
        "contents",
        "is_resource",
        "open_resource",
        "resource_path",
    },
    "OxidizedZipFinder": {
        "create_module",
        "exec_module",
        "find_module",
        "find_spec",
        "from_path",
        "from_zip_data",
        "get_code",
        "get_source",
        "invalidate_caches",
        "is_package",
    },
    "PythonExtensionModule": {"name"},
    "PythonModuleBytecode": {
        "bytecode",
        "is_package",
        "module",
        "optimize_level",
    },
    "PythonModuleSource": {"is_package", "module", "source"},
    "PythonPackageDistributionResource": {"data", "name", "package", "version"},
    "PythonPackageResource": {"data", "name", "package"},
    "decode_source": set(),
    "find_resources_in_path": set(),
    "pkg_resources_find_distributions": set(),
    "register_pkg_resources": set(),
}

COMMON_CLASS_DUNDER_ATTRIBUTES = {
    "__class__",
    "__delattr__",
    "__dir__",
    "__doc__",
    "__eq__",
    "__format__",
    "__ge__",
    "__getattribute__",
    "__gt__",
    "__hash__",
    "__init_subclass__",
    "__init__",
    "__le__",
    "__lt__",
    "__module__",
    "__ne__",
    "__reduce__",
    "__reduce_ex__",
    "__repr__",
    "__setattr__",
    "__sizeof__",
    "__str__",
    "__subclasshook__",
    "__new__",
}

COMMON_FUNCTION_DUNDER_ATTRIBUTES = {
    "__call__",
    "__class__",
    "__delattr__",
    "__dir__",
    "__doc__",
    "__eq__",
    "__format__",
    "__ge__",
    "__getattribute__",
    "__gt__",
    "__hash__",
    "__init__",
    "__init_subclass__",
    "__le__",
    "__lt__",
    "__module__",
    "__name__",
    "__ne__",
    "__new__",
    "__qualname__",
    "__reduce__",
    "__reduce_ex__",
    "__repr__",
    "__self__",
    "__setattr__",
    "__sizeof__",
    "__str__",
    "__subclasshook__",
    "__text_signature__",
}


class TestImporterModule(unittest.TestCase):
    def test_module(self):
        import oxidized_importer as importer

        attrs = {a for a in dir(importer) if not a.startswith("__")}
        self.assertEqual(
            attrs, set(SYMBOL_ATTRIBUTES.keys()), "module symbols match expected"
        )

    def test_symbol_attrs(self):
        import oxidized_importer as importer

        for (symbol, expected) in sorted(SYMBOL_ATTRIBUTES.items()):
            o = getattr(importer, symbol)

            if symbol.lower() == symbol:
                extra = COMMON_FUNCTION_DUNDER_ATTRIBUTES
            else:
                extra = COMMON_CLASS_DUNDER_ATTRIBUTES

            expected = extra | expected

            attrs = set(dir(o))
            self.assertEqual(attrs, expected, "attributes on %s" % symbol)


if __name__ == "__main__":
    unittest.main()
