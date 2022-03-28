default:
  cargo build

actions-install-sccache-linux:
  python3 scripts/secure_download.py \
    https://github.com/mozilla/sccache/releases/download/v0.2.15/sccache-v0.2.15-x86_64-unknown-linux-musl.tar.gz \
    e5d03a9aa3b9fac7e490391bbe22d4f42c840d31ef9eaf127a03101930cbb7ca \
    sccache.tar.gz
  tar -xvzf sccache.tar.gz
  mv sccache-v0.2.15-x86_64-unknown-linux-musl/sccache /home/runner/.cargo/bin/sccache
  rm -rf sccache*
  chmod +x /home/runner/.cargo/bin/sccache

actions-install-sccache-macos:
  python3 scripts/secure_download.py \
    https://github.com/mozilla/sccache/releases/download/v0.2.15/sccache-v0.2.15-x86_64-apple-darwin.tar.gz \
    908e939ea3513b52af03878753a58e7c09898991905b1ae3c137bb8f10fa1be2 \
    sccache.tar.gz
  tar -xvzf sccache.tar.gz
  mv sccache-v0.2.15-x86_64-apple-darwin/sccache /Users/runner/.cargo/bin/sccache
  rm -rf sccache*
  chmod +x /Users/runner/.cargo/bin/sccache

actions-install-sccache-windows:
  vcpkg integrate install
  vcpkg install openssl:x64-windows
  cargo install --features s3 --version 0.2.15 sccache

actions-bootstrap-rust-linux: actions-install-sccache-linux

actions-bootstrap-rust-macos: actions-install-sccache-macos
