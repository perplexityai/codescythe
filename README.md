# Codescythe

Codescythe is a focused TypeScript dead-code analyzer and remover inspired by
[Knip](https://knip.dev), scoped to entry/project graph analysis and unused
TypeScript exports/files. It intentionally avoids Knip's framework plugin
surface.

It exists for TypeScript codebases that want a smaller, more predictable cleanup
tool: start from known entry points, follow the import/export graph, and identify
project files or exported symbols that nothing reachable uses. Many dead-code
tools grow into broad framework integration layers; Codescythe chooses a narrower
contract so the analysis is easier to reason about, test, and run as part of
automated cleanup.

The goal is not to replace Knip for every framework-aware audit. Codescythe is
for the common package and monorepo maintenance job where the project boundary is
already known and the useful answer is deterministic: which TypeScript files and
exports are unused, and which of those removals can be applied safely.

## Codescythe And Knip

Codescythe takes a deliberately smaller slice of Knip's problem space.

| | Knip | Codescythe |
| --- | --- | --- |
| Primary scope | Broad JavaScript and TypeScript project hygiene: unused files, exports, dependencies, binaries, unresolved imports, and related issue types. | Focused TypeScript dead-code analysis: unused project files, unused exports, unresolved imports, and supported export removals. |
| Project discovery | Infers more from package metadata, workspaces, scripts, framework config, and built-in plugins. | Starts from explicit `entry` and `project` config, then follows the import/export graph. |
| Framework awareness | Designed for framework and tool integrations through plugins and compilers. | Intentionally avoids a framework plugin surface. |
| Best fit | Comprehensive audits where framework config, dependency hygiene, and workspace conventions matter. | Deterministic cleanup jobs where the TypeScript boundary is already known and repeatable graph behavior matters more than integration breadth. |

## Benchmarks

The benchmark suite runs Codescythe and Knip against pinned real-world
TypeScript-heavy repositories fetched through Bazel. A local smoke run with
`--samples 1 --warmups 0` produced:

| Fixture | Benchmarked TS-family files | Codescythe | Knip |
| --- | ---: | ---: | ---: |
| `microsoft/vscode` | 9,537 | 738.0ms | 5.76s |
| `grafana/grafana` | 8,701 | 771.0ms | 9.29s |
| `elastic/kibana` | 86,370 | 5.41s | 68.06s |

Counts reflect the shared benchmark config after excluding declaration files and
fixture directories. Run `pnpm benchmark` to measure the same fixtures locally.

## Config

The config schema lives at `codescythe.schema.json` and is compiled into the
core crate. Config can be provided as:

- `codescythe.json` in the project root.
- A `codescythe` object in `package.json`.
- An explicit path passed with `--config`.

Supported config fields are `entry`, `project`, `ignore`,
`includeEntryExports`, and `ignoreExportsUsedInFile`.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the repository layout, architecture,
build graph, benchmarks, release artifacts, and local validation commands.
