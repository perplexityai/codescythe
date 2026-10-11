#!/usr/bin/env bash
set -euo pipefail
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform=linux-amd64 ;;
  Darwin-arm64) platform=darwin-arm64 ;;
  *) echo 'Unsupported smoke platform' >&2; exit 1 ;;
esac
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
buildkite-agent artifact download 'release/*' "$work" \
  --build "${BUILDKITE_BUILD_ID:?}" --step release-build
python3 .buildkite/verify-artifact.py "$work/release"
binary="$work/release/codescythe-$platform"
chmod +x "$binary"
file "$binary"
"$binary" --version
"$binary" --help
set +e
output=$("$binary" --json -C tests/fixtures/knip-export-basics)
status=$?
set -e
[[ "$status" == 1 ]] || { echo "expected fixture analysis to exit with 1, got $status" >&2; exit 1; }
printf '%s\n' "$output" | grep '"dangling.ts"'
if [[ "$platform" == linux-* ]]; then
  ldd_output=$(ldd "$binary" 2>&1 || true)
  if ! printf '%s\n' "$ldd_output" | grep -Eq 'not a dynamic executable|statically linked'; then
    if strings "$binary" | grep 'GLIBC_'; then
      echo "$binary unexpectedly references glibc symbols" >&2
      exit 1
    fi
  fi
fi
