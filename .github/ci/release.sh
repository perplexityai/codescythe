#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  build)
    source .github/ci/setup.sh
    export CARGO_HOME="$CI_CACHE_DIR/cargo"
    export RUSTUP_HOME="$CI_CACHE_DIR/rustup"
    export PATH="$CARGO_HOME/bin:$PATH"
    download_verified \
      https://static.rust-lang.org/rustup/archive/1.28.2/x86_64-unknown-linux-gnu/rustup-init \
      "$CI_CACHE_DIR/bin/rustup-init" \
      20a06e644b0d9bd2fbdbfd52d42540bdde820ea7df86e92e533c073da0cdd43c
    chmod +x "$CI_CACHE_DIR/bin/rustup-init"
    "$CI_CACHE_DIR/bin/rustup-init" -y --no-modify-path --default-toolchain none
    rustup toolchain install 1.93.0 --profile minimal
    if ! command -v cc >/dev/null; then
      apt-get update
      DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends build-essential
    fi
    bazel build \
      //crates/codescythe_cli:release_binary_darwin_arm64 \
      //crates/codescythe_cli:release_binary_linux_amd64 \
      //crates/codescythe_cli:release_binary_linux_arm64 \
      //crates/codescythe_cli:release_binary_windows_amd64
    bazel test //crates/codescythe:codescythe_schema_test //crates/codescythe:codescythe_test
    rustup run 1.93.0 cargo package -p codescythe --locked
    mkdir -p release
    for platform in darwin_arm64 linux_amd64 linux_arm64 windows_amd64; do
      binary=$(bazel cquery --output=files "//crates/codescythe_cli:release_binary_$platform" | tail -n 1)
      name="codescythe-${platform//_/-}"
      [[ "$platform" != windows_amd64 ]] || name+=.exe
      cp "$binary" "release/$name"
    done
    version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' crates/codescythe/Cargo.toml)
    cp "target/package/codescythe-$version.crate" release/
    git archive --format=tar.gz --prefix="codescythe-$version/" \
      --output="release/codescythe-$version.tar.gz" HEAD
    python3 - <<'PY'
import hashlib
import json
import os
from pathlib import Path
import subprocess

root = Path("release")
files = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(root.iterdir())}
(root / "checksums.txt").write_text("".join(f"{digest}  {name}\n" for name, digest in files.items()))
files["checksums.txt"] = hashlib.sha256((root / "checksums.txt").read_bytes()).hexdigest()
(root / "gha-release.json").write_text(json.dumps({
    "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
    "build": os.environ["CI_RUN_URL"],
    "build_id": os.environ["CI_RUN_ID"],
    "pipeline": os.environ["GITHUB_REPOSITORY"],
    "files": files,
}, indent=2) + "\n")
PY
    ;;
  *) echo 'Expected build' >&2; exit 1 ;;
esac
