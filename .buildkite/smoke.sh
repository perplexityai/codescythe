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
"$binary" --version
"$binary" --help
set +e
output=$("$binary" --json -C tests/fixtures/knip-export-basics)
status=$?
set -e
[[ "$status" == 1 ]] || { echo "expected fixture analysis to exit with 1, got $status" >&2; exit 1; }
printf '%s\n' "$output" | grep '"dangling.ts"'
if [[ "$platform" == linux-* ]]; then
  python3 - "$binary" <<'PYTHON'
from pathlib import Path
import struct
import sys

binary = Path(sys.argv[1]).read_bytes()
if binary[:6] != b'\x7fELF\x02\x01' or struct.unpack_from('<H', binary, 18)[0] != 62:
    raise SystemExit('Expected Linux AMD64 ELF binary')
offset = struct.unpack_from('<Q', binary, 32)[0]
size, count = struct.unpack_from('<HH', binary, 54)
for index in range(count):
    if struct.unpack_from('<I', binary, offset + index * size)[0] == 3:
        raise SystemExit('CLI unexpectedly requires a dynamic interpreter')
print('Verified static Linux AMD64 binary')
PYTHON
fi
