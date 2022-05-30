default:
  cargo build

exe_suffix := if os() == "windows" { ".exe" } else { "" }

macosx_deployment_target := if os() == "macos" {
  if arch() == "arm" {
    "11.0"
  } else {
    "10.9"
  }
} else {
  ""
}

actions-install-sccache-linux:
  python3 scripts/secure_download.py \
    https://github.com/mozilla/sccache/releases/download/v0.3.0/sccache-v0.3.0-x86_64-unknown-linux-musl.tar.gz \
    e6cd8485f93d683a49c83796b9986f090901765aa4feb40d191b03ea770311d8 \
    sccache.tar.gz
  tar -xvzf sccache.tar.gz
  mv sccache-v0.3.0-x86_64-unknown-linux-musl/sccache /home/runner/.cargo/bin/sccache
  rm -rf sccache*
  chmod +x /home/runner/.cargo/bin/sccache

actions-install-sccache-macos:
  python3 scripts/secure_download.py \
    https://github.com/mozilla/sccache/releases/download/v0.3.0/sccache-v0.3.0-x86_64-apple-darwin.tar.gz \
    61c16fd36e32cdc923b66e4f95cb367494702f60f6d90659af1af84c3efb11eb \
    sccache.tar.gz
  tar -xvzf sccache.tar.gz
  mv sccache-v0.3.0-x86_64-apple-darwin/sccache /Users/runner/.cargo/bin/sccache
  rm -rf sccache*
  chmod +x /Users/runner/.cargo/bin/sccache

actions-install-sccache-windows:
  python3 scripts/secure_download.py \
    https://github.com/mozilla/sccache/releases/download/v0.3.0/sccache-v0.3.0-x86_64-pc-windows-msvc.tar.gz \
    f25e927584d79d0d5ad489e04ef01b058dad47ef2c1633a13d4c69dfb83ba2be \
    sccache.tar.gz
  tar -xvzf sccache.tar.gz
  mv sccache-v0.3.0-x86_64-pc-windows-msvc/sccache.exe C:/Users/runneradmin/.cargo/bin/sccache.exe

actions-bootstrap-rust-linux: actions-install-sccache-linux
  sudo apt install -y --no-install-recommends libpcsclite-dev musl-tools

actions-bootstrap-rust-macos: actions-install-sccache-macos

actions-bootstrap-rust-windows: actions-install-sccache-windows

actions-macos-universal exe:
  mkdir -p uploads
  lipo {{exe}}-x86-64/{{exe}} {{exe}}-aarch64/{{exe}} -create -output uploads/{{exe}}
  chmod +x uploads/{{exe}}
  lipo uploads/{{exe}} -info

actions-build-pyoxy-linux target_triple python_version:
  mkdir -p pyoxy/build target
  chmod 777 pyoxy/build target
  docker run \
    --rm \
    -v $(pwd):/pyoxidizer \
    -v /usr/local/bin/pyoxidizer:/usr/bin/pyoxidizer \
    pyoxidizer:build \
    /pyoxidizer/ci/build-pyoxy-linux.sh {{target_triple}} {{python_version}} build

  mkdir upload
  cp pyoxy/build/{{target_triple}}/release/pyoxy upload/
  cp pyoxy/build/{{target_triple}}/release/resources/COPYING.txt upload/COPYING

# Build PyOxy binary on Linux (local dev mode).
pyoxy-build-linux target_triple python_version:
  #!/usr/bin/env bash
  set -eo pipefail

  (cd ci && docker build -f linux-portable-binary.Dockerfile -t linux-portable-binary:latest .)
  cargo build --bin pyoxidizer --target x86_64-unknown-linux-musl

  rm -rf target/docker
  mkdir -p target/docker

  docker run \
    --rm \
    -it \
    -v $(pwd):/pyoxidizer \
    -v $(pwd)/target/x86_64-unknown-linux-musl/debug/pyoxidizer:/usr/bin/pyoxidizer \
    linux-portable-binary:latest \
    /pyoxidizer/ci/build-pyoxy-linux.sh {{target_triple}} {{python_version}} ../target/docker

pyoxy-build-linux-stage pyoxy_version triple python_version:
  #!/usr/bin/env bash
  set -eo pipefail

  just pyoxy-build-linux {{triple}} {{python_version}}

  DEST_DIR=dist/pyoxy-{{pyoxy_version}}-{{triple}}-python{{python_version}}
  mkdir -p ${DEST_DIR}

  cp target/docker/{{triple}}/release/pyoxy ${DEST_DIR}/
  cp target/docker/{{triple}}/release/resources/COPYING.txt ${DEST_DIR}/COPYING

# Build all PyOxy binaries for Linux.
pyoxy-build-linux-all:
  #!/usr/bin/env bash
  set -eo pipefail

  PYOXY_VERSION=$(cargo metadata --manifest-path pyoxy/Cargo.toml --no-deps | jq --raw-output '.packages[] | select(.name == "pyoxy") | .version')

  just pyoxy-build-linux-stage ${PYOXY_VERSION} x86_64-unknown-linux-gnu 3.8
  just pyoxy-build-linux-stage ${PYOXY_VERSION} x86_64-unknown-linux-gnu 3.9
  just pyoxy-build-linux-stage ${PYOXY_VERSION} x86_64-unknown-linux-gnu 3.10

actions-build-pyoxy-macos triple python_version:
  #!/usr/bin/env bash
  set -euxo pipefail

  export SDKROOT=/Applications/Xcode_13.2.1.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX12.1.sdk
  export MACOSX_DEPLOYMENT_TARGET={{macosx_deployment_target}}
  pyoxidizer build --release --target-triple {{triple}} --path pyoxy --var PYTHON_VERSION {{python_version}}
  PYO3_CONFIG_FILE=$(pwd)/pyoxy/build/{{triple}}/release/resources/pyo3-build-config-file.txt cargo build --bin pyoxy --target {{triple}} --release

  mkdir upload
  cp target/{{triple}}/release/pyoxy upload/
  sccache --stop-server

_remote-sign-exe ref workflow run_id artifact exe_name rcodesign_branch="main":
  gh workflow run sign-apple-exe.yml \
    --ref {{ref}} \
    -f workflow={{workflow}} \
    -f run_id={{run_id}} \
    -f artifact={{artifact}} \
    -f exe_name={{exe_name}} \
    -f rcodesign_branch={{rcodesign_branch}}

# Trigger remote code signing workflow for rcodesign executable.
remote-sign-rcodesign ref run_id rcodesign_branch="main": (_remote-sign-exe ref "rcodesign.yml" run_id "exe-rcodesign-macos-universal" "rcodesign" rcodesign_branch)
