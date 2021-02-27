# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import unittest


class TestImporterModule(unittest.TestCase):
    def test_module(self):
        import oxidized_importer as importer

        attrs = {a for a in dir(importer) if not a.startswith("__")}
        self.assertEqual(
            attrs,
            {
                "decode_source",
                "find_resources_in_path",
                "OxidizedDistribution",
                "OxidizedFinder",
                "OxidizedResourceCollector",
                "OxidizedResourceReader",
                "OxidizedResource",
                "PythonExtensionModule",
                "PythonModuleBytecode",
                "PythonModuleSource",
                "PythonPackageDistributionResource",
                "PythonPackageResource",
            },
        )

    def test_finder_attrs(self):
        from oxidized_importer import OxidizedFinder

        attrs = {a for a in dir(OxidizedFinder) if not a.startswith("__")}
        self.assertEqual(
            attrs,
            {
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
                "serialize_indexed_resources",
            },
        )


if __name__ == "__main__":
    unittest.main()
