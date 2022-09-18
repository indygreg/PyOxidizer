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
  #!/usr/bin/env bash
  set -eo pipefail

  mkdir -p uploads
  lipo {{exe}}-x86-64/{{exe}} {{exe}}-aarch64/{{exe}} -create -output uploads/{{exe}}
  chmod +x uploads/{{exe}}
  lipo uploads/{{exe}} -info

  # There might be a COPYING file with licensing info. If so, preserve it.
  if [ -e "{{exe}}-aarch64/COPYING" ]; then
    cp -v {{exe}}-aarch64/COPYING uploads/
  fi

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
  cp pyoxy/build/{{triple}}/release/resources/COPYING.txt upload/COPYING
  sccache --stop-server

# Trigger a workflow on a branch.
ci-run workflow branch="ci-test":
  gh workflow run {{workflow}} --ref {{branch}}

# Trigger all workflows on a given branch.
ci-run-all branch="ci-test":
  just ci-run cargo_deny.yml {{branch}}
  just ci-run oxidized_importer.yml {{branch}}
  just ci-run pyoxidizer.yml {{branch}}
  just ci-run pyoxy.yml {{branch}}
  just ci-run sphinx.yml {{branch}}
  just ci-run workspace.yml {{branch}}
  just ci-run workspace-python.yml {{branch}}

_remote-sign-exe ref workflow run_id artifact exe_name rcodesign_branch="main":
  gh workflow run sign-apple-exe.yml \
    --ref {{ref}} \
    -f workflow={{workflow}} \
    -f run_id={{run_id}} \
    -f artifact={{artifact}} \
    -f exe_name={{exe_name}} \
    -f rcodesign_branch={{rcodesign_branch}}

# Trigger remote code signing workflow for pyoxy executable.
remote-sign-pyoxy ref run_id rcodesign_branch="main": (_remote-sign-exe ref "rcodesign.yml" run_id "exe-pyoxy-macos-universal" "pyoxy" rcodesign_branch)

# Obtain built executables from GitHub Actions.
assemble-exe-artifacts exe commit dest:
  #!/usr/bin/env bash
  set -exo pipefail

  RUN_ID=$(gh run list \
    --workflow {{exe}}.yml \
    --json databaseId,headSha | \
    jq --raw-output '.[] | select(.headSha=="{{commit}}") | .databaseId' | head -n 1)

  if [ -z "${RUN_ID}" ]; then
    echo "could not find GitHub Actions run with artifacts"
    exit 1
  fi

  echo "GitHub run ID: ${RUN_ID}"

  gh run download --dir {{dest}} ${RUN_ID}

_codesign-exe in_path:
  rcodesign sign \
    --remote-signer \
    --remote-public-key-pem-file ci/developer-id-application.pem \
    --code-signature-flags runtime \
    {{in_path}}

_codesign in_path out_path:
  rcodesign sign \
    --remote-signer \
    --remote-public-key-pem-file ci/developer-id-application.pem \
    {{in_path}} {{out_path}}

# Notarize and staple a path.
notarize path:
  rcodesign notarize \
    --api-issuer 254e4e96-2b8b-43c1-b385-286bdad51dba \
    --api-key 8RXL6MN9WV \
    --staple \
    {{path}}

_tar_directory source_directory dir_name dest_dir:
  tar \
    --sort=name \
    --owner=root:0 \
    --group=root:0 \
    --mtime="2022-01-01 00:00:00" \
    -C {{source_directory}} \
    -cvzf {{dest_dir}}/{{dir_name}}.tar.gz \
    {{dir_name}}/

_zip_directory source_directory dir_name dest_dir:
  #!/usr/bin/env bash
  set -exo pipefail

  here=$(pwd)

  cd {{source_directory}}
  zip -r ${here}/{{dest_dir}}/{{dir_name}}.zip {{dir_name}}

_release_universal_binary project tag exe:
  mkdir -p dist/{{project}}-stage/{{project}}-{{tag}}-macos-universal
  llvm-lipo-14 \
    -create \
    -output dist/{{project}}-stage/{{project}}-{{tag}}-macos-universal/{{exe}} \
    dist/{{project}}-stage/{{project}}-{{tag}}-aarch64-apple-darwin/{{exe}} \
    dist/{{project}}-stage/{{project}}-{{tag}}-x86_64-apple-darwin/{{exe}}
  cp dist/{{project}}-stage/{{project}}-{{tag}}-aarch64-apple-darwin/COPYING \
    dist/{{project}}-stage/{{project}}-{{tag}}-macos-universal/COPYING

_create_shasums dir:
  #!/usr/bin/env bash
  set -exo pipefail

  (cd {{dir}} && shasum -a 256 *.* > SHA256SUMS)

  for p in {{dir}}/*.*; do
    if [[ "${p}" != *"SHA256SUMS" ]]; then
      shasum -a 256 $p | awk '{print $1}' > ${p}.sha256
    fi
  done

_upload_release name title_name commit tag:
  git tag -f {{name}}/{{tag}} {{commit}}
  git push -f origin refs/tags/{{name}}/{{tag}}:refs/tags/{{name}}/{{tag}}
  gh release create \
    --prerelease \
    --target {{commit}} \
    --title '{{title_name}} {{tag}}' \
    --discussion-category general \
    {{name}}/{{tag}}
  gh release upload --clobber {{name}}/{{tag}} dist/{{name}}/*

_release name title_name:
  #!/usr/bin/env bash
  set -exo pipefail

  COMMIT=$(git rev-parse HEAD)
  TAG=$(cargo metadata \
    --manifest-path {{name}}/Cargo.toml \
    --format-version 1 \
    --no-deps | \
      jq --raw-output '.packages[] | select(.name=="{{name}}") | .version')

  just {{name}}-release-prepare ${COMMIT} ${TAG}
  just {{name}}-release-upload ${COMMIT} ${TAG}

# Prepare PyOxy release artifacts.
pyoxy-release-prepare commit tag:
  #!/usr/bin/env bash
  set -exo pipefail

  rm -rf dist/pyoxy*
  just assemble-exe-artifacts pyoxy {{commit}} dist/pyoxy-artifacts

  for py in 3.8 3.9 3.10; do
    for triple in aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu macos-universal; do
      release_name=pyoxy-{{tag}}-${triple}-cpython${py}
      source=dist/pyoxy-artifacts/exe-pyoxy-${triple}-${py}
      dest=dist/pyoxy-stage/${release_name}
      mkdir -p ${dest}
      cp -a ${source}/pyoxy ${dest}/pyoxy
      # GitHub Actions zip files don't preserve executable bit.
      chmod +x ${dest}/pyoxy
      cp -a ${source}/COPYING ${dest}/COPYING

      case ${triple} in
        *apple* | macos-universal)
          just _codesign-exe ${dest}/pyoxy
          ;;
        *)
          ;;
      esac

      mkdir -p dist/pyoxy

      just _tar_directory \
        dist/pyoxy-stage \
        ${release_name} \
        dist/pyoxy
    done
  done

  just _create_shasums dist/pyoxy

# Upload PyOxy release artifacts to a new GitHub release.
pyoxy-release-upload commit tag:
  just _upload_release pyoxy PyOxy {{commit}} {{tag}}

# Perform a PyOxy release end-to-end.
pyoxy-release:
  just _release pyoxy PyOxy

oxidized_importer-release-prepare commit tag:
  #!/usr/bin/env bash
  set -exo pipefail

  rm -rf dist/oxidized_importer*

  just assemble-exe-artifacts oxidized_importer {{commit}} dist/oxidized_importer-artifacts
  mkdir dist/oxidized_importer
  cp dist/oxidized_importer-artifacts/wheels/*.whl dist/oxidized_importer

  just _create_shasums dist/oxidized_importer

oxidized_importer-release-upload commit tag:
  just _upload_release oxidized_importer 'Oxidized Importer Python Extension' {{commit}} {{tag}}
  twine upload dist/oxidized_importer/*.whl

oxidized_importer-release commit tag:
  just oxidized_importer-release-prepare {{commit}} {{tag}}
  just oxidized_importer-release-upload {{commit}} {{tag}}

# Create a .dmg for PyOxidizer
pyoxidizer-create-dmg:
  #!/usr/bin/env bash
  set -exo pipefail

  # Clear out old state.
  rm -rf build dmg_root PyOxidizer.dmg dist/pyoxidizer*

  if [ -d /Volumes/PyOxidizer ]; then
    DEV_NAME=$(hdiutil info | egrep --color=never '^/dev/' | sed 1q | awk '{print $1}')
    hdiutil detach "${DEV_NAME}"
  fi

  if [[ $(uname -m) == 'arm64' ]]; then
    PYOXIDIZER=target/aarch64-apple-darwin/release/pyoxidizer
  else
    PYOXIDIZER=target/x86_64-apple-darwin/release/pyoxidizer
  fi

  $PYOXIDIZER build --release macos_app_bundle
  just _codesign build/*/release/macos_app_bundle/PyOxidizer.app dist/pyoxidizer-stage/PyOxidizer.app

  hdiutil create \
    -srcfolder dist/pyoxidizer-stage \
    -volname PyOxidizer \
    -fs HFS+ \
    -fsargs "-c c=64,a=16,e=16" \
    -format UDRW \
    PyOxidizer

  # Mount it.
  DEV_NAME=$(hdiutil attach -readwrite -noverify -noautoopen PyOxidizer.dmg | egrep --color=never '^/dev/' | sed 1q | awk '{print $1}')

  # Create a symlink to /Applications for drag and drop.
  ln -s /Applications /Volumes/PyOxidizer/Applications

  # Run AppleScript to create the .DS_Store.
  /usr/bin/osascript scripts/dmg.applescript PyOxidizer

  # --openfolder not supported on ARM.
  if [[ $(uname -m) == "arm64" ]]; then
    bless --folder /Volumes/PyOxidizer
  else
    bless --folder /Volumes/PyOxidizer --openfolder /Volumes/PyOxidizer
  fi

  # Unmount.
  hdiutil detach "${DEV_NAME}"

  # Compress.
  hdiutil convert PyOxidizer.dmg -format UDZO -imagekey zlib-level=9 -ov -o PyOxidizer.dmg
  just _codesign PyOxidizer.dmg PyOxidizer.dmg
  just notarize PyOxidizer.dmg

# Prepare PyOxidizer release artifacts.
pyoxidizer-release-prepare commit tag:
  #!/usr/bin/env bash
  set -exo pipefail

  rm -rf dist/pyoxidizer*

  just assemble-exe-artifacts pyoxidizer {{commit}} dist/pyoxidizer-artifacts

  mkdir dist/pyoxidizer dist/pyoxidizer-stage

  # Windows installers are easy.
  cp -av dist/pyoxidizer-artifacts/pyoxidizer-windows_installers/*.{exe,msi} dist/pyoxidizer/

  # Assemble plain executable releases.
  for triple in aarch64-apple-darwin aarch64-unknown-linux-musl i686-pc-windows-msvc x86_64-apple-darwin x86_64-pc-windows-msvc x86_64-unknown-linux-musl; do
    release_name=pyoxidizer-{{tag}}-${triple}
    source=dist/pyoxidizer-artifacts/exe-pyoxidizer-${triple}
    dest=dist/pyoxidizer-stage/${release_name}

    exe=pyoxidizer
    sign_command=
    archive_action=_tar_directory

    case ${triple} in
      *apple*)
        sign_command="just _codesign-exe ${dest}/${exe}"
        ;;
      *windows*)
        exe=pyoxidizer.exe
        archive_action=_zip_directory
        ;;
      *)
        ;;
    esac

    mkdir -p ${dest}
    cp -a ${source}/${exe} ${dest}/${exe}
    chmod +x ${dest}/${exe}

    if [ -n "${sign_command}" ]; then
      ${sign_command}
    fi

    cargo run --bin pyoxidizer -- rust-project-licensing \
      --system-rust \
      --target-triple ${triple} \
      --all-features \
      --unified-license \
      pyoxidizer > ${dest}/COPYING

    just ${archive_action} dist/pyoxidizer-stage ${release_name} dist/pyoxidizer

    # Wheels are built using the (signed) executables in the archives.
    mkdir -p target/${triple}/release
    cp ${dest}/${exe} target/${triple}/release/
    cargo run --bin pyoxidizer -- build --release wheel_${triple}

    cp build/*/release/wheel_${triple}/*.whl dist/pyoxidizer/
  done

  # Create universal binary from signed single arch Mach-O binaries.
  just _release_universal_binary pyoxidizer {{tag}} pyoxidizer
  just _tar_directory dist/pyoxidizer-stage pyoxidizer-{{tag}}-macos-universal dist/pyoxidizer

  # The DMG is created using the signed binaries.
  for triple in aarch64-apple-darwin x86_64-apple-darwin; do
    ssh macmini mkdir -p /Users/gps/src/PyOxidizer/target/${triple}/release
    scp dist/pyoxidizer-stage/pyoxidizer-{{tag}}-${triple}/pyoxidizer macmini:~/src/PyOxidizer/target/${triple}/release/
  done
  ssh macmini just -d /Users/gps/src/PyOxidizer -f /Users/gps/src/PyOxidizer/Justfile pyoxidizer-create-dmg
  scp macmini:~/src/PyOxidizer/PyOxidizer.dmg dist/pyoxidizer/PyOxidizer-{{tag}}.dmg

  just _create_shasums dist/pyoxidizer

# Upload PyOxidizer release artifacts.
pyoxidizer-release-upload commit tag:
  just _upload_release pyoxidizer PyOxidizer {{commit}} {{tag}}
  twine upload dist/pyoxidizer/*.whl

# Perform release automation for PyOxidizer.
pyoxidizer-release:
  just _release pyoxidizer 'PyOxidizer'
