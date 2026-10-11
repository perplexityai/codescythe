#!/usr/bin/env bash
set -euo pipefail
[[ "$RELEASE_TAG" =~ ^codescythe_cli_v[0-9]+\.[0-9]+\.[0-9]+([-.][A-Za-z0-9.-]+)?$ ]]
mkdir -p release
for attempt in $(seq 1 270); do
  if gh release download "$RELEASE_TAG" --pattern buildkite-release.json --dir release --clobber; then
    break
  fi
  [[ "$attempt" != 270 ]] || { echo 'Buildkite release artifacts missing after 90 minutes' >&2; exit 1; }
  sleep 20
done
version=${RELEASE_TAG#codescythe_cli_v}
for asset in codescythe-darwin-arm64 codescythe-linux-amd64 codescythe-linux-arm64 codescythe-windows-amd64.exe \
  "codescythe-$version.crate" "codescythe-$version.tar.gz" checksums.txt; do
  gh release download "$RELEASE_TAG" --pattern "buildkite-$asset" --dir release --clobber
  mv "release/buildkite-$asset" "release/$asset"
done
python3 - <<'PY'
import hashlib
import json
import os
from pathlib import Path
import subprocess

root = Path("release")
manifest = json.loads((root / "buildkite-release.json").read_text())
commit = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
if manifest["commit"] != commit:
    raise SystemExit("Release artifacts do not match checked-out commit")
version = os.environ["RELEASE_TAG"].removeprefix("codescythe_cli_v")
expected = {"codescythe-darwin-arm64", "codescythe-linux-amd64", "codescythe-linux-arm64", "codescythe-windows-amd64.exe",
            f"codescythe-{version}.crate", f"codescythe-{version}.tar.gz", "checksums.txt"}
if set(manifest["files"]) != expected:
    raise SystemExit("Unexpected release artifact set")
for name, digest in manifest["files"].items():
    if hashlib.sha256((root / name).read_bytes()).hexdigest() != digest:
        raise SystemExit(f"Release artifact checksum mismatch: {name}")
print(f"Verified artifacts for {commit}: {manifest['build']}")
PY
