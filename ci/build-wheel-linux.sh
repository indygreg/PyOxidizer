#!/usr/bin/env bash
# Script to build Linux binary wheel for oxidized_importer.

set -exo pipefail

export CIBW_ENVIRONMENT='PATH="$PATH:$HOME/.cargo/bin"'
export CIBW_BEFORE_BUILD=ci/install-rust-linux.sh
export CIBW_BUILD=cp38-manylinux_x86_64

python3.8 -m cibuildwheel --output-dir wheelhouse --platform linux
