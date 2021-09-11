# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import importlib
import importlib.machinery
import importlib.util
import io
import marshal
import os
import pathlib
import struct
import sys
import tempfile
import time
import unittest
import zipfile

from oxidized_importer import OxidizedZipFinder

DEFAULT_MTIME = 1631383005


def make_zip(files, prefix=None, compression=zipfile.ZIP_DEFLATED):
    """Obtain zip file data from file descriptions."""
    b = io.BytesIO()

    if prefix:
        b.write(prefix)

    with zipfile.ZipFile(b, "w") as zf:
        for name, (mtime, data) in sorted(files.items()):
            zi = zipfile.ZipInfo(name, time.localtime(mtime))
            zi.compress_type = compression
            zf.writestr(zi, data)

    return b.getvalue()


def make_pyc(code, mtime, size):

    return b"%s%s%s" % (
        importlib.util.MAGIC_NUMBER,
        struct.pack("<iLL", 0, mtime & 0xFFFFFFFF, size & 0xFFFFFFFF),
        marshal.dumps(code),
    )


class TestImporter(unittest.TestCase):
    def setUp(self):
        self.path = sys.path[:]
        self.meta_path = sys.meta_path[:]
        self.path_hooks = sys.path_hooks[:]
        sys.path_importer_cache.clear()
        self.modules_before = sys.modules.copy()
        self.raw_temp_dir = tempfile.TemporaryDirectory(
            prefix="oxidized_importer-test-"
        )
        self.td = pathlib.Path(self.raw_temp_dir.name)

    def tearDown(self):
        self.raw_temp_dir.cleanup()
        sys.path[:] = self.path
        sys.meta_path[:] = self.meta_path
        sys.path_hooks[:] = self.path_hooks
        sys.path_importer_cache.clear()
        sys.modules.clear()
        sys.modules.update(self.modules_before)

    def test_missing(self):
        zip_data = make_zip({"foo.py": (DEFAULT_MTIME, b"print('hello, world')\n")})
        importer = OxidizedZipFinder.from_zip_data(zip_data)

        self.assertIsNone(importer.find_module("missing"))

        with self.assertRaises(ImportError):
            importer.get_code("missing")

        with self.assertRaises(ImportError):
            importer.get_source("missing")

        with self.assertRaises(ImportError):
            importer.is_package("missing")

    def test_import_py_only(self):
        source = b"foo = 42\n"

        zip_data = make_zip({"foo.py": (DEFAULT_MTIME, source)})

        importer = OxidizedZipFinder.from_zip_data(zip_data)

        spec = importer.find_spec("foo", None)
        self.assertIsInstance(spec, importlib.machinery.ModuleSpec)
        self.assertEqual(spec.name, "foo")
        self.assertIsInstance(spec.loader, OxidizedZipFinder)
        self.assertEqual(spec.origin, os.path.join(sys.executable, "foo.py"))
        self.assertIsNone(spec.loader_state)
        self.assertIsNone(spec.submodule_search_locations)

        self.assertEqual(importer.find_module("foo"), importer)

        wanted_code = compile(source.decode("ascii"), "foo.py", "exec")

        self.assertEqual(importer.get_code("foo"), wanted_code)
        self.assertEqual(importer.get_source("foo"), source.decode("ascii"))
        self.assertFalse(importer.is_package("foo"))

        sys.meta_path.insert(0, importer)
        m = importlib.import_module("foo")

        self.assertIsNotNone(m)
        self.assertEqual(m.__name__, "foo")
        self.assertIsInstance(m.__loader__, OxidizedZipFinder)
        self.assertEqual(m.__loader__, importer)
        self.assertEqual(m.__package__, "")
        self.assertEqual(m.__file__, spec.origin)
        self.assertIsNotNone(m.__cached__)

    def test_import_pyc_only(self):
        source = b"foo = 42\n"
        code = compile(source.decode("ascii"), "foo.py", "exec")
        bytecode = make_pyc(code, DEFAULT_MTIME, len(source))

        zip_data = make_zip({"foo.pyc": (DEFAULT_MTIME, bytecode)})

        importer = OxidizedZipFinder.from_zip_data(zip_data)

        spec = importer.find_spec("foo", None)
        self.assertIsInstance(spec, importlib.machinery.ModuleSpec)
        self.assertEqual(spec.name, "foo")
        self.assertIsInstance(spec.loader, OxidizedZipFinder)
        self.assertEqual(spec.origin, os.path.join(sys.executable, "foo.pyc"))
        self.assertIsNone(spec.loader_state)
        self.assertIsNone(spec.submodule_search_locations)

        self.assertEqual(importer.find_module("foo"), importer)

        self.assertEqual(importer.get_code("foo"), code)
        self.assertIsNone(importer.get_source("foo"))
        self.assertFalse(importer.is_package("foo"))

        sys.meta_path.insert(0, importer)
        m = importlib.import_module("foo")

        self.assertIsNotNone(m)
        self.assertEqual(m.__name__, "foo")
        self.assertIsInstance(m.__loader__, OxidizedZipFinder)
        self.assertEqual(m.__loader__, importer)
        self.assertEqual(m.__package__, "")
        self.assertEqual(m.__file__, spec.origin)
        self.assertIsNotNone(m.__cached__)

    def test_import_py_and_pyc(self):
        source = b"foo = 42\n"
        code = compile(source.decode("ascii"), "foo.py", "exec")
        bytecode = make_pyc(code, DEFAULT_MTIME, len(source))

        zip_data = make_zip(
            {"foo.py": (DEFAULT_MTIME, source), "foo.pyc": (DEFAULT_MTIME, bytecode)}
        )

        importer = OxidizedZipFinder.from_zip_data(zip_data)

        spec = importer.find_spec("foo", None)
        self.assertIsInstance(spec, importlib.machinery.ModuleSpec)
        self.assertEqual(spec.name, "foo")
        self.assertIsInstance(spec.loader, OxidizedZipFinder)
        self.assertEqual(spec.origin, os.path.join(sys.executable, "foo.py"))
        self.assertIsNone(spec.loader_state)
        self.assertIsNone(spec.submodule_search_locations)

        self.assertEqual(importer.find_module("foo"), importer)

        self.assertEqual(importer.get_code("foo"), code)
        self.assertEqual(importer.get_source("foo"), source.decode("ascii"))
        self.assertFalse(importer.is_package("foo"))

        sys.meta_path.insert(0, importer)
        m = importlib.import_module("foo")

        self.assertIsNotNone(m)
        self.assertEqual(m.__name__, "foo")
        self.assertIsInstance(m.__loader__, OxidizedZipFinder)
        self.assertEqual(m.__loader__, importer)
        self.assertEqual(m.__package__, "")
        self.assertEqual(m.__file__, spec.origin)
        self.assertIsNotNone(m.__cached__)

    def test_import_package_py_only(self):
        source = b"foo = 42\n"

        zip_data = make_zip({"foo/__init__.py": (DEFAULT_MTIME, source)})

        importer = OxidizedZipFinder.from_zip_data(zip_data)

        spec = importer.find_spec("foo", None)
        self.assertIsInstance(spec, importlib.machinery.ModuleSpec)
        self.assertEqual(spec.name, "foo")
        self.assertIsInstance(spec.loader, OxidizedZipFinder)
        self.assertEqual(
            spec.origin, os.path.join(sys.executable, "foo", "__init__.py")
        )
        self.assertIsNone(spec.loader_state)
        self.assertEqual(
            spec.submodule_search_locations, [os.path.join(sys.executable, "foo")]
        )

        self.assertEqual(importer.find_module("foo"), importer)

        wanted_code = compile(source.decode("ascii"), "foo.py", "exec")

        self.assertEqual(importer.get_code("foo"), wanted_code)
        self.assertEqual(importer.get_source("foo"), source.decode("ascii"))
        self.assertTrue(importer.is_package("foo"))

        sys.meta_path.insert(0, importer)
        m = importlib.import_module("foo")

        self.assertIsNotNone(m)
        self.assertEqual(m.__name__, "foo")
        self.assertIsInstance(m.__loader__, OxidizedZipFinder)
        self.assertEqual(m.__loader__, importer)
        self.assertEqual(m.__package__, "foo")
        self.assertEqual(m.__file__, spec.origin)
        self.assertIsNotNone(m.__cached__)

    def test_import_package_pyc_only(self):
        source = b"foo = 42\n"
        code = compile(source.decode("ascii"), "foo.py", "exec")
        bytecode = make_pyc(code, DEFAULT_MTIME, len(source))

        zip_data = make_zip({"foo/__init__.pyc": (DEFAULT_MTIME, bytecode)})

        importer = OxidizedZipFinder.from_zip_data(zip_data)

        spec = importer.find_spec("foo", None)
        self.assertIsInstance(spec, importlib.machinery.ModuleSpec)
        self.assertEqual(spec.name, "foo")
        self.assertIsInstance(spec.loader, OxidizedZipFinder)
        self.assertEqual(
            spec.origin, os.path.join(sys.executable, "foo", "__init__.pyc")
        )
        self.assertIsNone(spec.loader_state)
        self.assertEqual(
            spec.submodule_search_locations, [os.path.join(sys.executable, "foo")]
        )

        self.assertEqual(importer.find_module("foo"), importer)

        self.assertEqual(importer.get_code("foo"), code)
        self.assertIsNone(importer.get_source("foo"))
        self.assertTrue(importer.is_package("foo"))

        sys.meta_path.insert(0, importer)
        m = importlib.import_module("foo")

        self.assertIsNotNone(m)
        self.assertEqual(m.__name__, "foo")
        self.assertIsInstance(m.__loader__, OxidizedZipFinder)
        self.assertEqual(m.__loader__, importer)
        self.assertEqual(m.__package__, "foo")
        self.assertEqual(m.__file__, spec.origin)
        self.assertIsNotNone(m.__cached__)

    def test_import_package_py_and_pyc(self):
        source = b"foo = 42\n"
        code = compile(source.decode("ascii"), "foo.py", "exec")
        bytecode = make_pyc(code, DEFAULT_MTIME, len(source))

        zip_data = make_zip(
            {
                "foo/__init__.py": (DEFAULT_MTIME, source),
                "foo/__init__.pyc": (DEFAULT_MTIME, bytecode),
            }
        )

        importer = OxidizedZipFinder.from_zip_data(zip_data)

        spec = importer.find_spec("foo", None)
        self.assertIsInstance(spec, importlib.machinery.ModuleSpec)
        self.assertEqual(spec.name, "foo")
        self.assertIsInstance(spec.loader, OxidizedZipFinder)
        self.assertEqual(
            spec.origin, os.path.join(sys.executable, "foo", "__init__.py")
        )
        self.assertIsNone(spec.loader_state)
        self.assertEqual(
            spec.submodule_search_locations, [os.path.join(sys.executable, "foo")]
        )

        self.assertEqual(importer.find_module("foo"), importer)

        self.assertEqual(importer.get_code("foo"), code)
        self.assertEqual(importer.get_source("foo"), source.decode("ascii"))
        self.assertTrue(importer.is_package("foo"))

        sys.meta_path.insert(0, importer)
        m = importlib.import_module("foo")

        self.assertIsNotNone(m)
        self.assertEqual(m.__name__, "foo")
        self.assertIsInstance(m.__loader__, OxidizedZipFinder)
        self.assertEqual(m.__loader__, importer)
        self.assertEqual(m.__package__, "foo")
        self.assertEqual(m.__file__, spec.origin)
        self.assertIsNotNone(m.__cached__)

    def test_import_submodule_py_only(self):
        source = b"foo = 42\n"

        zip_data = make_zip(
            {
                "foo/__init__.py": (DEFAULT_MTIME, b""),
                "foo/bar.py": (DEFAULT_MTIME, source),
            }
        )

        importer = OxidizedZipFinder.from_zip_data(zip_data)

        spec = importer.find_spec("foo.bar", None)
        self.assertIsInstance(spec, importlib.machinery.ModuleSpec)
        self.assertEqual(spec.name, "foo.bar")
        self.assertIsInstance(spec.loader, OxidizedZipFinder)
        self.assertEqual(spec.origin, os.path.join(sys.executable, "foo", "bar.py"))
        self.assertIsNone(spec.loader_state)
        self.assertIsNone(spec.submodule_search_locations)
        self.assertEqual(importer.find_module("foo.bar"), importer)

        wanted_code = compile(source.decode("ascii"), "bar.py", "exec")

        self.assertEqual(importer.get_code("foo.bar"), wanted_code)
        self.assertEqual(importer.get_source("foo.bar"), source.decode("ascii"))
        self.assertFalse(importer.is_package("foo.bar"))

        sys.meta_path.insert(0, importer)
        m = importlib.import_module("foo.bar")

        self.assertIsNotNone(m)
        self.assertEqual(m.__name__, "foo.bar")
        self.assertIsInstance(m.__loader__, OxidizedZipFinder)
        self.assertEqual(m.__loader__, importer)
        self.assertEqual(m.__package__, "foo")
        self.assertEqual(m.__file__, spec.origin)
        self.assertIsNotNone(m.__cached__)

    def test_zip_file(self):
        source = b"foo = 42\n"

        zip_data = make_zip({"foo.py": (DEFAULT_MTIME, source)})

        p = self.td / "test.zip"

        with p.open("wb") as fh:
            fh.write(zip_data)

        importer = OxidizedZipFinder.from_path(p)
        spec = importer.find_spec("foo", None)

        self.assertEqual(spec.origin, str(p / "foo.py"))
        self.assertIsNone(spec.submodule_search_locations)


if __name__ == "__main__":
    unittest.main()
