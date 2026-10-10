"""Compare packaged inputs before publishing without compiling again."""

import sys
import tarfile


def contents(path):
    with tarfile.open(path, "r:gz") as archive:
        files = {}
        for member in archive.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or member.name in files:
                raise SystemExit(f"Unexpected crate archive entry: {member.name}")
            files[member.name] = archive.extractfile(member).read()
        return files


if contents(sys.argv[1]) != contents(sys.argv[2]):
    raise SystemExit("Cargo package differs from Buildkite-verified crate")
print("Cargo package matches Buildkite-verified crate")
