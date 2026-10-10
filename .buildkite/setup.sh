#!/usr/bin/env bash
set -euo pipefail

export CI_CACHE_DIR=${CI_CACHE_DIR:-/tmp/codescythe-ci}
export USE_BAZEL_VERSION=${USE_BAZEL_VERSION:-$(cat .bazelversion)}
export BAZELISK_HOME="$CI_CACHE_DIR/bazelisk"
export RUNNER_TEMP=${RUNNER_TEMP:-$PWD/.buildkite-artifacts}
mkdir -p "$RUNNER_TEMP"
export GITHUB_WORKSPACE="$PWD"
mkdir -p "$CI_CACHE_DIR/bin"
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)
    platform=linux-amd64
    bazelisk_sha=5a408715e932c0250d28bd84555f12edbf70117de42f9181691c736eacc4a992
    node_platform=linux-x64
    node_archive=tar.xz
    node_sha=bdebee276e58d0ef5448f3d5ac12c67daa963dd5e0a9bb621a53d1cefbc852fd
    export RUNNER_OS=Linux
    ;;
  Linux-aarch64)
    platform=linux-arm64
    bazelisk_sha=e20e8b0f4f240091b7a55bf17b9398bd4f40ee70ae0208dff95dd4c445fb4010
    node_platform=linux-arm64
    node_archive=tar.xz
    node_sha=a06d42807fb500f7459e5f3fa6cb431447352826ee6f07e14adfeec58a1b3210
    export RUNNER_OS=Linux
    ;;
  Darwin-arm64)
    platform=darwin-arm64
    bazelisk_sha=cee851f726789227d5561004e9904a52be45c3efb56f8b38b6993d6adbaa0409
    node_platform=darwin-arm64
    node_archive=tar.gz
    node_sha=319f221adc5e44ff0ed57e8a441b2284f02b8dc6fc87b8eb92a6a93643fd8080
    export RUNNER_OS=macOS
    ;;
  *) echo 'Unsupported agent platform' >&2; exit 1 ;;
esac

download_verified() {
  local url=$1 output=$2 sha=$3
  if ! echo "$sha  $output" | shasum -a 256 --check --status 2>/dev/null; then
    curl --fail --silent --show-error --location --retry 3 "$url" --output "$output"
    echo "$sha  $output" | shasum -a 256 --check
  fi
}

download_verified "https://github.com/bazelbuild/bazelisk/releases/download/v1.29.0/bazelisk-$platform" \
  "$CI_CACHE_DIR/bin/bazelisk-real" "$bazelisk_sha"
chmod +x "$CI_CACHE_DIR/bin/bazelisk-real"
cat > "$CI_CACHE_DIR/bin/bazelisk" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
startup=()
while [[ $# -gt 0 && "$1" == -* ]]; do
  startup+=("$1")
  shift
done
[[ $# -gt 0 ]] || exec "$CI_CACHE_DIR/bin/bazelisk-real" "${startup[@]}"
command=$1
shift
case "$command" in
  build|test|run|coverage|fetch)
    contents_cache=()
    # Bazel 9 repository contents can retain LLVM symlinks into old agents.
    if [[ "${USE_BAZEL_VERSION%%.*}" -ge 9 ]]; then
      contents_cache=(--repo_contents_cache=)
    fi
    exec "$CI_CACHE_DIR/bin/bazelisk-real" "${startup[@]}" "$command" \
      --repository_cache="$CI_CACHE_DIR/repository" \
      --disk_cache="$CI_CACHE_DIR/disk/$USE_BAZEL_VERSION" "${contents_cache[@]}" "$@"
    ;;
  *) exec "$CI_CACHE_DIR/bin/bazelisk-real" "${startup[@]}" "$command" "$@" ;;
esac
SH
chmod +x "$CI_CACHE_DIR/bin/bazelisk"
ln -sf bazelisk "$CI_CACHE_DIR/bin/bazel"
export PATH="$CI_CACHE_DIR/bin:$PATH"

install_node() {
  local archive="$CI_CACHE_DIR/node.$node_archive"
  download_verified "https://nodejs.org/dist/v24.12.0/node-v24.12.0-$node_platform.$node_archive" \
    "$archive" "$node_sha"
  tar -xf "$archive" -C "$CI_CACHE_DIR"
  export PATH="$CI_CACHE_DIR/node-v24.12.0-$node_platform/bin:$PATH"
}

install_dependencies() {
  install_node
  if [[ -f pnpm-lock.yaml ]]; then
    local manager
    manager=$(node -p 'require("./package.json").packageManager')
    [[ "$manager" =~ ^pnpm@[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
      echo 'Pin packageManager to a pnpm version' >&2; exit 1;
    }
    npm install --global --prefix "$CI_CACHE_DIR/pnpm" "$manager"
    export PATH="$CI_CACHE_DIR/pnpm/bin:$PATH"
    pnpm install --frozen-lockfile
  else
    npm ci
  fi
}
