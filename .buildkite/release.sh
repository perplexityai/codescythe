#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  build)
    source .buildkite/setup.sh
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
      //crates/codescythe_cli:release_binary_linux_arm64
    bazel test //crates/codescythe:codescythe_schema_test //crates/codescythe:codescythe_test
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
    "build_id": os.environ["BUILDKITE_BUILD_ID"],
    "pipeline": os.environ["BUILDKITE_PIPELINE_SLUG"],
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
    buildkite-agent artifact download 'release/*' . --build "${BUILDKITE_BUILD_ID:?}" --step release-build
    python3 .buildkite/verify-artifact.py release
    response=$(curl --fail --silent --show-error --request POST \
      --header "Authorization: Token $BUILDKITE_AGENT_ACCESS_TOKEN" \
      --header 'Content-Type: application/json' \
      --data "{\"repo_url\":\"$BUILDKITE_REPO\",\"workflow\":\"cli-release.yml\",\"permissions\":{\"contents\":\"write\"}}" \
      "$BUILDKITE_AGENT_ENDPOINT/jobs/$BUILDKITE_JOB_ID/github_workflow_access_token")
    printf '%s' "$response" | buildkite-agent redactor add --format json
    GH_TOKEN=$(printf '%s' "$response" | jq --exit-status --raw-output .token)
    export GH_TOKEN GH_REPO=perplexityai/codescythe
    unset response
    version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' crates/codescythe/Cargo.toml)
    [[ "$BUILDKITE_TAG" == "codescythe_cli_v$version" ]]
    python3 - <<'PYTHON'
import json
import os
from pathlib import Path
import time
import urllib.error
import urllib.parse
import urllib.request

repo = "perplexityai/codescythe"
tag = os.environ["BUILDKITE_TAG"]
headers = {"Authorization": f"Bearer {os.environ['GH_TOKEN']}",
           "Accept": "application/vnd.github+json", "User-Agent": "codescythe-buildkite-release"}

def request(url, method="GET", data=None, content_type="application/json"):
    req = urllib.request.Request(url, method=method, data=data,
                                 headers={**headers, "Content-Type": content_type})
    with urllib.request.urlopen(req, timeout=120) as response:
        body = response.read()
        return json.loads(body) if body else None

# Release Please can create the tag before the release exists.
for attempt in range(60):
    try:
        release = request(f"https://api.github.com/repos/{repo}/releases/tags/{tag}")
        break
    except urllib.error.HTTPError as error:
        if error.code != 404 or attempt == 59:
            raise
        time.sleep(10)
upload_url = release["upload_url"].split("{")[0]
if urllib.parse.urlparse(upload_url).hostname != "uploads.github.com":
    raise SystemExit("Unexpected GitHub release upload endpoint")
existing = {asset["name"]: asset["id"] for asset in release["assets"]}
# Remove the old completion marker before replacing any staged asset.
marker = existing.pop("buildkite-release.json", None)
if marker is not None:
    request(f"https://api.github.com/repos/{repo}/releases/assets/{marker}", "DELETE")
paths = sorted(Path("release").iterdir())
# Completion manifest goes last; readers never consume partial uploads.
paths.sort(key=lambda path: path.name == "buildkite-release.json")
for path in paths:
    name = path.name if path.name == "buildkite-release.json" else f"buildkite-{path.name}"
    if name in existing:
        request(f"https://api.github.com/repos/{repo}/releases/assets/{existing[name]}", "DELETE")
    request(f"{upload_url}?{urllib.parse.urlencode({'name': name})}", "POST",
            path.read_bytes(), "application/octet-stream")
    print(f"Uploaded {name}")
PYTHON
    ;;
  *) echo 'Expected build or upload' >&2; exit 1 ;;
esac
