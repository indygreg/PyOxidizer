# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import distutils.command.build_ext
import distutils.extension
import os
import pathlib
import re
import setuptools
import shutil
import subprocess
import sys

HERE = pathlib.Path(os.path.dirname(os.path.abspath(__file__)))


class RustExtension(distutils.extension.Extension):
    def __init__(self, name):
        super().__init__(name, [])

        self.depends.extend(
            [HERE / "Cargo.toml", "src/lib.rs",]
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

        subprocess.run(args, env=env, check=True)

        dest_path = get_ext_path_fn(self.name)
        suffix = pathlib.Path(dest_path).suffix

        rust_lib_filename = "lib%s%s" % (self.name, suffix)
        rust_lib = build_dir / "release" / rust_lib_filename

        shutil.copy2(rust_lib, dest_path)


class RustBuildExt(distutils.command.build_ext.build_ext):
    def build_extension(self, ext):
        assert isinstance(ext, RustExtension)

        ext.build(
            build_dir=pathlib.Path(self.build_temp),
            get_ext_path_fn=self.get_ext_fullpath,
        )


def get_version():
    cargo_toml = HERE / "Cargo.toml"

    with cargo_toml.open("r", encoding="utf-8") as fh:
        for line in fh:
            m = re.match('^version = "([^"]+)"', line)
            if m:
                return m.group(1)

    raise Exception("could not find version string")


# TODO check for Python 3.8.
setuptools.setup(
    name="oxidized_importer",
    version=get_version(),
    author="Gregory Szorc",
    author_email="gregory.szorc@gmail.com",
    url="https://github.com/indygreg/PyOxidizer",
    description="Python importer implemented in Rust",
    license="MPL 2.0",
    classifiers=["Intended Audience :: Developers", "Programming Language :: Rust",],
    ext_modules=[RustExtension("oxidized_importer")],
    cmdclass={"build_ext": RustBuildExt},
)
