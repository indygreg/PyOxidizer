# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""This setup.py is for the oxidized_importer Python extension module.

It should exist in the `oxidzed-importer` directory. But since it needs
to pull in sources from outside that directory and `pip` can be opinionated
about not allowing that, the file exists in the root of the repository to
work around this limitation.
"""

import distutils.command.build_ext
import distutils.extension
import os
import pathlib
import setuptools
import shutil
import subprocess
import sys

HERE = pathlib.Path(os.path.dirname(os.path.abspath(__file__)))
OXIDIZED_IMPORTER = HERE / "oxidized-importer"


class RustExtension(distutils.extension.Extension):
    def __init__(self, name):
        super().__init__(name, [])

        self.depends.extend(
            [OXIDIZED_IMPORTER / "Cargo.toml", OXIDIZED_IMPORTER / "src/lib.rs"]
        )

    def build(self, build_dir: pathlib.Path, get_ext_path_fn):
        env = os.environ.copy()
        env["PYTHON_SYS_EXECUTABLE"] = sys.executable

        args = [
            "cargo",
            "build",
            "--release",
            "--target-dir",
            str(build_dir),
        ]

        subprocess.run(args, env=env, cwd=OXIDIZED_IMPORTER, check=True)

        dest_path = pathlib.Path(get_ext_path_fn(self.name))

        if os.name == "nt":
            rust_lib_filename = "%s.dll" % self.name
        elif sys.platform == "darwin":
            rust_lib_filename = "lib%s.dylib" % self.name
        else:
            rust_lib_filename = "lib%s.so" % self.name

        rust_lib = build_dir / "release" / rust_lib_filename

        dest_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(rust_lib, dest_path)


class RustBuildExt(distutils.command.build_ext.build_ext):
    def build_extension(self, ext):
        assert isinstance(ext, RustExtension)

        ext.build(
            build_dir=pathlib.Path(os.path.abspath(self.build_temp)),
            get_ext_path_fn=self.get_ext_fullpath,
        )


with open("oxidized-importer/README.md", "r", encoding="utf-8") as fh:
    long_description = fh.read()


setuptools.setup(
    name="oxidized_importer",
    version="0.2dev0",
    author="Gregory Szorc",
    author_email="gregory.szorc@gmail.com",
    url="https://github.com/indygreg/PyOxidizer",
    description="Python importer implemented in Rust",
    long_description=long_description,
    license="MPL 2.0",
    python_requires=">=3.8",
    classifiers=["Intended Audience :: Developers", "Programming Language :: Rust",],
    ext_modules=[RustExtension("oxidized_importer")],
    cmdclass={"build_ext": RustBuildExt},
)
