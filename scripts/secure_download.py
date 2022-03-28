#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Securely download a file by validating its SHA-256 against expectations.

import argparse
import gzip
import hashlib
import http.client
import pathlib
import urllib.error
import urllib.request


def hash_path(p: pathlib.Path):
    h = hashlib.sha256()

    with p.open("rb") as fh:
        while True:
            chunk = fh.read(65536)
            if not chunk:
                break

            h.update(chunk)

    return h.hexdigest()


class IntegrityError(Exception):
    """Represents an integrity error when downloading a URL."""


def secure_download_stream(url, sha256):
    """Securely download a URL to a stream of chunks.

    If the integrity of the download fails, an exception is raised.
    """
    h = hashlib.sha256()

    with urllib.request.urlopen(url) as fh:
        if not url.endswith(".gz") and fh.info().get("Content-Encoding") == "gzip":
            fh = gzip.GzipFile(fileobj=fh)

        while True:
            chunk = fh.read(65536)
            if not chunk:
                break

            h.update(chunk)

            yield chunk

    digest = h.hexdigest()

    if digest != sha256:
        raise IntegrityError(
            "integrity mismatch on %s: wanted sha256=%s; got sha256=%s"
            % (url, sha256, digest)
        )


def download_to_path(url: str, path: pathlib.Path, sha256: str):
    # We download to a temporary file and rename at the end so there's
    # no chance of the final file being partially written or containing
    # bad data.
    print("downloading %s to %s" % (url, path))

    if path.exists():
        good = True

        if good:
            if hash_path(path) != sha256:
                print("existing file hash is wrong; removing")
                good = False

        if good:
            print("%s exists and passes integrity checks" % path)
            return

        path.unlink()

    tmp = path.with_name("%s.tmp" % path.name)

    path.parent.mkdir(parents=True, exist_ok=True)

    for _ in range(5):
        try:
            try:
                with tmp.open("wb") as fh:
                    for chunk in secure_download_stream(url, sha256):
                        fh.write(chunk)

                break
            except IntegrityError:
                tmp.unlink()
                raise
        except http.client.HTTPException as e:
            print("HTTP exception; retrying: %s" % e)
        except urllib.error.URLError as e:
            print("urllib error; retrying: %s" % e)
    else:
        raise Exception("download failed after multiple retries")

    tmp.rename(path)
    print("successfully downloaded %s" % url)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("url", help="URL to download")
    parser.add_argument("sha256", help="Expected SHA-256 of downloaded file")
    parser.add_argument("dest", help="Destination path to write")

    args = parser.parse_args()

    download_to_path(args.url, pathlib.Path(args.dest), sha256=args.sha256)


if __name__ == "__main__":
    main()
