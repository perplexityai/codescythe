# Codescythe

Codescythe is a focused TypeScript dead-code analyzer and remover inspired by
Knip, scoped to entry/project graph analysis and unused TypeScript exports/files.
It intentionally avoids Knip's framework plugin surface.

## Architecture

Codescythe is split into a small Rust analysis core, two runtime adapters, and
distribution packages. Source files are intentionally flattened at each crate or
package root instead of hidden under `src/` folders.

```text
.
|-- codescythe.schema.json
|-- crates/
|   |-- codescythe/            # Rust core analysis, config loading, fix logic
|   |-- codescythe_cli/        # Standalone CLI binary
|   `-- codescythe_napi/       # Node-API shared library adapter
|-- packages/
|   |-- codescythe/            # Public npm package and TypeScript loader
|   |-- codescythe-darwin-arm64/
|   |-- codescythe-linux-amd64/
|   `-- codescythe-linux-arm64/
|-- tests/
|   `-- fixtures/              # Knip-style conformance fixtures
`-- tools/
    `-- ts.bzl                 # Minimal Gazelle TS mapping
```

### Core Crate

`crates/codescythe` owns the analyzer. It loads `codescythe.json` or the
`codescythe` key in `package.json`, validates that config with the bundled JSON
Schema, walks the configured project globs, parses TypeScript/JavaScript with
Oxc, builds the import/export graph, and reports unused files, unused exports,
and unresolved imports.

The public Rust API is intentionally narrow:

- `codescythe::run(cwd, config_path)` returns an analysis report.
- `codescythe::run_and_fix(cwd, config_path)` applies supported removals and
  returns a fix report.

The core crate has no npm or CLI concerns. That keeps conformance tests and
future analysis work centered on one library boundary.

### Runtime Adapters

`crates/codescythe_cli` is a thin `clap` wrapper around the core crate. It
supports text and JSON output, exits with `1` when issues are found, and exits
with `2` for runtime/config errors.

`crates/codescythe_napi` exposes the same core behavior to Node through N-API.
It returns JSON strings from Rust, while the public TypeScript loader parses
those strings into JavaScript objects.

### Npm Package Boundary

The pnpm workspace treats `packages/*` as public distribution boundaries. The
root `package.json` owns workspace imports and scripts; public packages own
their own `package.json` files.

`@perplexity/codescythe` is the public npm package. Its TypeScript loader chooses
one optional native package from `process.platform` and `process.arch`:

- `@perplexity/codescythe-darwin-arm64`
- `@perplexity/codescythe-linux-amd64`
- `@perplexity/codescythe-linux-arm64`

The package entrypoints are TypeScript files and are executed with Node's
`--experimental-transform-types` support. The package CLI shim is also
TypeScript.

### Build Graph

Bazel is the source of truth for release artifacts.

```text
//crates/codescythe
  |-- used by //crates/codescythe_cli:codescythe
  `-- used by //crates/codescythe_napi:codescythe_napi

//crates/codescythe_cli:release_binaries
  |-- codescythe-darwin-arm64
  |-- codescythe-linux-amd64   # musl static/static-pie
  `-- codescythe-linux-arm64   # musl static

//crates/codescythe_napi:release_nodes
  |-- codescythe.darwin-arm64.node
  |-- codescythe.linux-amd64.node   # musl shared object
  `-- codescythe.linux-arm64.node   # musl shared object

//packages/...:package
  `-- copies the matching TypeScript loader/package files plus native output
```

The release transitions in `crates/codescythe_cli/release_binary.bzl` and
`crates/codescythe_napi/release_node.bzl` use `with_cfg` to force optimized
platform builds and select the LLVM cross toolchains. Linux CLI binaries use
musl targets for static artifacts. Linux N-API packages also use musl targets,
with `crt-static` disabled for the shared-library build so Rust emits a `.node`
shared object instead of dropping the `cdylib` output.

### Config

The config schema lives at `codescythe.schema.json` and is compiled into the
core crate. Config can be provided as:

- `codescythe.json` in the project root.
- A `codescythe` object in `package.json`.
- An explicit path passed with `--config`.

Supported config fields are `entry`, `project`, `ignore`,
`includeEntryExports`, and `ignoreExportsUsedInFile`.

### Tests And CI

The Rust conformance test lives in `crates/codescythe` and uses the
`tests/fixtures/knip-export-basics` fixture. Npm smoke coverage is colocated with
the public package in `packages/codescythe/npm_smoke.ts` and runs through Mocha.

GitHub Actions builds all release targets on macOS, uploads the package and
binary artifacts, then smoke-tests each triple on its native runner:

- Darwin arm64 on `macos-15`.
- Linux amd64 on `ubuntu-24.04`.
- Linux arm64 on `ubuntu-24.04-arm`.

The smoke jobs verify the npm package loader, direct native package loading, the
package CLI shim, the standalone static binary, fixture output, and that Linux
artifacts do not reference `GLIBC_` symbols.

## Development

```sh
bazel test //...
bazel run //:gazelle
cargo test
```

Build release artifacts:

```sh
bazel build //crates/codescythe_cli:release_binaries
bazel build //crates/codescythe_napi:release_nodes
bazel build //packages/codescythe:package //packages/codescythe-darwin-arm64:package //packages/codescythe-linux-amd64:package //packages/codescythe-linux-arm64:package
```

Run the colocated npm smoke test against unpacked package artifacts by setting
`CODESCYTHE_PACKAGE_DIR` and `CODESCYTHE_NATIVE_PACKAGE_DIR`, then running:

```sh
pnpm test:npm
```
