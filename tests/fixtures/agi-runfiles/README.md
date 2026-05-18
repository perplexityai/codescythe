# AGI Runfiles Fixture

This fixture mirrors the Codescythe shape AGI needs under Bazel runfiles: the
CLI is pointed at a root config file, root `package.json#imports` are available,
explicit aliases can override package imports, generated namespaces can be
ignored, and runtime-only leaves remain explicit entries.
