# CI

GitHub Actions runs the previous Buildkite test matrix, examples, cross-builds,
and native artifact smoke tests. `CI` fails if any job fails or skips.

Artifacts stay inside one workflow run. Smoke jobs verify the commit, run attempt,
repository, platform, and SHA-256 before executing downloaded binaries.

Bazel caches are isolated by job and platform. Bazel 9 repository contents caches
stay disabled because LLVM repositories can retain absolute symlinks.

Releases require successful main CI at the exact tagged commit. Publishing stays
in GitHub Actions; no Buildkite credentials or staged assets are required.
