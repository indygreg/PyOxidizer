#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Script to download build artifacts from Azure Pipelines."""

import json
import io
import pathlib
import sys
import urllib.request
import zipfile


ARTIFACTS_URL = "https://dev.azure.com/gregoryszorc/PyOxidizer/_apis/build/builds/{build_id}/artifacts?api-version=4.1"


def main(build_id, dest_path):
    dest_path = pathlib.Path(dest_path)

    r = urllib.request.urlopen(ARTIFACTS_URL.format(build_id=build_id))

    res = json.load(r)

    for artifact in res["value"]:
        url = artifact["resource"]["downloadUrl"]

        zipdata = io.BytesIO(urllib.request.urlopen(url).read())

        with zipfile.ZipFile(zipdata, "r") as zf:
            for name in zf.namelist():
                name_path = pathlib.Path(name)
                dest = dest_path / name_path.name
                print("writing %s" % dest)

                with dest.open("wb") as fh:
                    fh.write(zf.read(name))


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("usage: download_artifacts.py <build_id> <dest_path>")
        sys.exit(1)

    sys.exit(main(*sys.argv[1:]))
