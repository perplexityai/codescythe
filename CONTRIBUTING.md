# Contributing

This document covers the repository details that matter when working on
Codescythe itself: the package layout, Rust boundaries, Bazel release graph, and
validation commands.

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
|   |-- codescythe/            # Public npm package and TypeScript source loader
|   |-- codescythe-darwin-arm64/
|   |-- codescythe-linux-amd64/
|   `-- codescythe-linux-arm64/
|-- tests/
|   `-- fixtures/              # Knip-style conformance fixtures
`-- tools/
    `-- ts.bzl                 # Minimal Gazelle TS mapping
```

The root `codescythe.schema.json` is the canonical editor/npm-facing schema.
`//crates/codescythe:codescythe_schema` uses `write_source_file` to keep the
crate-local `crates/codescythe/codescythe.schema.json` copy in sync for Cargo
packaging, where the crate tarball cannot read files outside `crates/codescythe`.

### Core Crate

`crates/codescythe` owns the analyzer. It loads `codescythe.json`,
`codescythe.jsonc`, or the `codescythe` key in `package.json`, validates that
config with the bundled JSON Schema, applies discovered `.gitignore` files
during project discovery, walks the configured project globs, parses
TypeScript/JavaScript with Oxc, builds the import/export graph, and reports
unused files, unused exports, and unresolved imports.

The public Rust API is intentionally narrow:

- `codescythe::run(cwd, config_path)` returns an analysis report.
- `codescythe::run_with_options(cwd, config_path, options)` enables verbose
  diagnostics and one-export explanations.
- `codescythe::run_and_fix(cwd, config_path)` applies supported removals and
  returns a fix report.
- `codescythe::doctor(cwd, config_path)` returns config-risk diagnostics.

The core crate has no npm or CLI concerns. That keeps conformance tests and
future analysis work centered on one library boundary.

### Runtime Adapters

`crates/codescythe_cli` is a thin `clap` wrapper around the core crate. It
supports text and JSON output, exits with `1` when issues are found, and exits
with `2` for runtime/config errors.

`crates/codescythe_napi` exposes the same core behavior to Node through N-API:
`analyze`, `fix`, and `doctor` all return JSON strings from Rust, while the
public package loader parses those strings into JavaScript objects.

### Npm Package Boundary

The pnpm workspace treats `packages/*` as public distribution boundaries. The
root `package.json` owns workspace imports and scripts; public packages own
their own `package.json` files.

`codescythe` is the public npm package. Its TypeScript source loader chooses one
optional native package from `process.platform` and `process.arch`:

- `codescythe-darwin-arm64`
- `codescythe-linux-amd64`
- `codescythe-linux-arm64`

Package manifests point at generated JavaScript entrypoints. The npm dist target
uses `ts_project` to compile the TypeScript package loaders to CommonJS `.js`
files so installed packages do not rely on Node type stripping under
`node_modules`.

## Build Graph

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
  |-- codescythe.linux-amd64.node   # GNU Linux shared object
  `-- codescythe.linux-arm64.node   # GNU Linux shared object

//:dist
  `-- writes a pnpm workspace containing JS package entrypoints and native output
```

The release transitions in `crates/codescythe_cli/release_binary.bzl` and
`crates/codescythe_napi/release_node.bzl` use `with_cfg` to force optimized
platform builds and select the LLVM cross toolchains. Linux CLI binaries use
musl targets for static artifacts. Linux N-API packages use the GNU Linux
targets because npm ships `.node` shared objects, and the npm release smoke test
uses `ldd` to reject missing runtime libraries.

## Development

```sh
bazel test //...
bazel run //:gazelle
cargo test
```

Repo-local Node tooling should be written as TypeScript and run with Node's
`--experimental-transform-types` support. Do not add `.mjs` tooling scripts.

Build release artifacts:

```sh
bazel build //crates/codescythe_cli:release_binaries
bazel build //crates/codescythe_napi:release_nodes
bazel build :dist
```

Run the colocated npm smoke test against unpacked package artifacts by setting
`CODESCYTHE_PACKAGE_DIR` and `CODESCYTHE_NATIVE_PACKAGE_DIR`, then running:

```sh
pnpm test:npm
```

## Benchmarks

The benchmark harness in `benchmarks/` uses pinned real-world source snapshots
from `microsoft/vscode`, `grafana/grafana`, `elastic/kibana`, and
`renovatebot/renovate`. The fixtures are declared as Bazel external
repositories in `MODULE.bazel`, and the harness uses the `benchmark` npm
package to time a release Codescythe CLI build and, when available, Knip with
issue reporting limited to files and exports.

```sh
pnpm benchmark
```

By default the benchmark fetches all fixtures through Bazel, builds
`codescythe` with Cargo before measuring, and compares against the workspace's
Knip dev dependency. Use `--fixture vscode`, `--fixture grafana`, or
`--fixture kibana`, or `--fixture renovate` to run one fixture. Set
`CODESCYTHE_BIN=/path/to/codescythe` or `KNIP_BIN=/path/to/knip` to benchmark a
specific binary. Pass `--skip-build` when `target/release/codescythe` already
exists, or `--skip-knip` to run only Codescythe.

Codescythe discovers the full project file set up front, then parses files in
parallel graph-frontier batches from the configured entries. Benchmarks whose
config treats every source file as an entry still exercise whole-corpus parsing;
entry-specific audits exercise the lazy path. Set `CODESCYTHE_PARSE_THREADS` to
tune parse parallelism; `RAYON_NUM_THREADS` is respected when the
Codescythe-specific variable is unset.

Vendored fixture conformance snapshots live under `benchmarks/` as
`*_conformance.snapshot.json`. Refresh them with
`bazel run //benchmarks:<fixture>_conformance` after reviewing an intentional
entry/config or analyzer result change, then verify the corresponding
`//benchmarks:<fixture>_conformance_test` target.

## Tests And CI

The Rust conformance test lives in `crates/codescythe` and uses the
`tests/fixtures/knip-export-basics` fixture. Npm smoke coverage is colocated with
the public package in `packages/codescythe/npm_smoke.ts` and runs through Mocha.

Pull requests run the default `test native` lane with
`bazel test //... --build_tests_only --test_tag_filters=-functional_test`.
The slower real-repo fixture checks are tagged `functional_test` and run from
the main-only `test functional` workflow.

GitHub Actions builds release binaries and npm package artifacts on
`ubuntu-24.04` with Bazel cross toolchains, uploads the artifacts, then
smoke-tests each triple on its native runner:

- Darwin arm64 on `macos-15`.
- Linux amd64 on `ubuntu-24.04`.
- Linux arm64 on `ubuntu-24.04-arm`.

The smoke jobs verify the npm package loader, direct native package loading, the
package CLI shim, the standalone static binary, fixture output, and that Linux
artifacts do not reference `GLIBC_` symbols.

Release Please owns version bumps for Cargo manifests, Cargo.lock entries, Bazel
`VERSION` constants, CLI e2e expectations, and npm package manifests. When it
creates a `codescythe_cli_v*` release, the CLI asset release, npm release, and
crates.io release workflows run from their `release.published` triggers. npm
publishing uses a temporary pnpm release workspace so `workspace:*` optional
dependencies publish as exact versions, and crates.io publishing uses trusted
publishing after a Bazel package input check plus `cargo publish --dry-run`.
