"""Verify release artifacts from this build before executing or uploading them."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def verify(root):
    marker = root / 'buildkite-release.json'
    if marker.is_symlink():
        raise ValueError('Unsafe artifact manifest')
    manifest = json.loads(marker.read_text())
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    expected = {'commit': commit, 'build_id': os.environ['BUILDKITE_BUILD_ID'],
                'pipeline': os.environ['BUILDKITE_PIPELINE_SLUG']}
    for key, value in expected.items():
        if manifest.get(key) != value:
            raise ValueError(f'Artifact {key} mismatch')
    if {path.name for path in root.iterdir()} != set(manifest['files']) | {'buildkite-release.json'}:
        raise ValueError('Unexpected artifact set')
    for name, digest in manifest['files'].items():
        if Path(name).name != name or name in {'.', '..'}:
            raise ValueError('Invalid artifact path')
        path = root / name
        if path.is_symlink() or not path.is_file():
            raise ValueError(f'Missing or unsafe artifact: {name}')
        with path.open('rb') as stream:
            checksum = hashlib.sha256()
            for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                checksum.update(chunk)
            actual = checksum.hexdigest()
        if actual != digest:
            raise ValueError(f'Artifact checksum mismatch: {name}')
    if not {'codescythe-linux-amd64', 'codescythe-linux-arm64',
            'codescythe-darwin-arm64', 'codescythe-windows-amd64.exe', 'checksums.txt'} <= manifest['files'].keys():
        raise ValueError('Missing CLI artifact set')


if __name__ == '__main__':
    verify(Path(sys.argv[1]))
