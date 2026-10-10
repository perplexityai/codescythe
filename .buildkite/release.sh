#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  build)
    source .buildkite/setup.sh
    bazel build \
      //crates/codescythe_cli:release_binary_darwin_arm64 \
      //crates/codescythe_cli:release_binary_linux_amd64 \
      //crates/codescythe_cli:release_binary_linux_arm64
    bazel test //crates/codescythe:codescythe_schema_test //crates/codescythe:codescythe_test
    rustup toolchain install 1.93.0 --profile minimal
    rustup run 1.93.0 cargo package -p codescythe --locked
    mkdir -p release
    for platform in darwin_arm64 linux_amd64 linux_arm64; do
      binary=$(bazel cquery --output=files "//crates/codescythe_cli:release_binary_$platform" | tail -n 1)
      cp "$binary" "release/codescythe-${platform//_/-}"
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
(root / "buildkite-release.json").write_text(json.dumps({
    "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
    "build": os.environ["BUILDKITE_BUILD_URL"],
    "files": files,
}, indent=2) + "\n")
PY
    ;;
  upload)
    [[ "${BUILDKITE_PULL_REQUEST:-false}" == false ]]
    [[ "${BUILDKITE_TAG:-}" =~ ^codescythe_cli_v[0-9]+\.[0-9]+\.[0-9]+([-.][A-Za-z0-9.-]+)?$ ]]
    git fetch origin main
    git merge-base --is-ancestor HEAD origin/main
    [[ "$(git rev-parse HEAD)" == "$BUILDKITE_COMMIT" ]]
    buildkite-agent artifact download 'release/*' . --step release-build
    response=$(curl --fail --silent --show-error --request POST \
      --header "Authorization: Token $BUILDKITE_AGENT_ACCESS_TOKEN" \
      --header 'Content-Type: application/json' \
      --data "{\"repo_url\":\"$BUILDKITE_REPO\",\"workflow\":\"cli-release.yml\",\"permissions\":{\"contents\":\"write\"}}" \
      "$BUILDKITE_AGENT_ENDPOINT/jobs/$BUILDKITE_JOB_ID/github_workflow_access_token")
    printf '%s' "$response" | buildkite-agent redactor add --format json
    GH_TOKEN=$(printf '%s' "$response" | jq --exit-status --raw-output .token)
    export GH_TOKEN GH_REPO=perplexityai/codescythe
    unset response
    # Release Please can create the tag before the release exists.
    for attempt in $(seq 1 60); do
      if gh release view "$BUILDKITE_TAG" >/dev/null 2>&1; then break; fi
      [[ "$attempt" != 60 ]] || { echo 'GitHub release missing' >&2; exit 1; }
      sleep 10
    done
    version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' crates/codescythe/Cargo.toml)
    [[ "$BUILDKITE_TAG" == "codescythe_cli_v$version" ]]
    mkdir -p staging
    # Completion manifest goes last; readers never consume partial uploads.
    for asset in release/*; do
      [[ "$asset" == release/buildkite-release.json ]] && continue
      name=${asset#release/}
      cp "$asset" "staging/buildkite-$name"
      gh release upload "$BUILDKITE_TAG" "staging/buildkite-$name" --clobber
    done
    gh release upload "$BUILDKITE_TAG" release/buildkite-release.json --clobber
    ;;
  *) echo 'Expected build or upload' >&2; exit 1 ;;
esac
