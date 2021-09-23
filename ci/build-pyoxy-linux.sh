#!/bin/bash

set -ex

cd /pyoxidizer
/opt/bin/pyoxidizer build --release --path pyoxy
export PYO3_CONFIG_FILE=$(pwd)/pyoxy/build/x86_64-unknown-linux-gnu/release/resources/pyo3-build-config-file.txt
~/.cargo/bin/cargo build --bin pyoxy --release
