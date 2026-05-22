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
| Primary scope | Broad JavaScript and TypeScript project hygiene: unused files, exports, dependencies, binaries, unresolved imports, and related issue types. | Focused TypeScript dead-code analysis: unused project files, unused exports, unresolved imports, and supported removals. |
| Project discovery | Infers more from package metadata, workspaces, scripts, framework config, and built-in plugins. | Starts from explicit `entry` and `project` config, then follows the import/export graph. |
| Framework awareness | Designed for framework and tool integrations through plugins and compilers. | Intentionally avoids a framework plugin surface. |
| Best fit | Comprehensive audits where framework config, dependency hygiene, and workspace conventions matter. | Deterministic cleanup jobs where the TypeScript boundary is already known and repeatable graph behavior matters more than integration breadth. |

## Benchmarks

The benchmark suite runs Codescythe and Knip against pinned real-world
TypeScript-heavy repositories fetched through Bazel. Representative local runs
produced:

| Fixture | Benchmarked files | Codescythe | Knip |
| --- | ---: | ---: | ---: |
| `microsoft/vscode` | 9,398 | 1.11s | 4.22s |
| `grafana/grafana` | 8,358 | 833.2ms | 9.51s |
| `elastic/kibana` | 85,928 | 12.96s | 53.33s |
| `renovatebot/renovate` | 2,456 | 154.5ms | 900.5ms |

Counts reflect each fixture's generated benchmark config after excludes. Run
`pnpm benchmark` to measure the same fixtures locally.

## Config

The config schema lives at `codescythe.schema.json` and is compiled into the
core crate. Config can be provided as:

- `codescythe.json` in the project root.
- `codescythe.jsonc` in the project root, when `codescythe.json` is absent.
- A `codescythe` object in `package.json`.
- An explicit `.json` or `.jsonc` path passed with `--config`.

Supported config fields are `entry`, `project`, `testFilePatterns`, `ignore`,
`aliases`, `unresolvedImports`, `includeEntryExports`, and
`ignoreExportsUsedInFile`. Codescythe automatically discovers `.gitignore` files
in every traversed directory.

Files matching `testFilePatterns` are treated as leaf files. By default this
includes `**/*.test.*`: those files are kept out of production usage marking,
but `--fix` can remove them when they import a project file or export that
Codescythe is removing. When a matching test imports live production source, its
project-file imports are also kept out of the unused-file report. `.spec.*`
files are not matched by default; model detached end-to-end specs as entries
instead.

Use `--verbose --json` when validating config changes or comparing runs. Verbose
analysis includes the Codescythe version, config path, project and entry counts,
package import keys, ignored unresolved-import patterns, source-alias ignore
warnings, and explanations for unused exports. Ignored unresolved imports are
grouped under `ignoredUnresolvedImportsByPattern` with sample specifiers and
importer files, so generated-import suppressions are visible instead of being
silent.

## Fixing

Run Codescythe with `--fix` to apply supported removals. The fix pass removes
unused project files and removes unused export declarations from reachable files.
The JSON fix report includes `removedFiles`, `changedFiles`, `removedExports`,
and the original analysis result.

`--fix` refuses source-like unresolved-import ignore patterns that overlap
package `imports` or configured source aliases unless `--force` is provided.
Extensionless and JS/TS-family patterns can hide real source imports, while
non-JS/TS asset patterns such as `*.svg?raw` still warn but do not block
`--fix`. When ignored unresolved imports create alias-namespace uncertainty for
a file, Codescythe skips export edits for that file and reports it in
`skippedExportFiles`.

Fixing is a single analysis-and-edit pass. Removing a dead file can make more
files or exports unreachable, so repeated cleanup jobs should run Codescythe
again after a fix pass when a completely stable tree is required.

## Explanations And Doctor

Use `--explain-export <file>:<symbol>` to ask why one export is dead or alive:

```sh
codescythe --explain-export src/constants.ts:getServerId
```

Use `doctor` to check config risk before running destructive fixes:

```sh
codescythe doctor --config codescythe.jsonc
```

The doctor flags broad unresolved-import ignores under local aliases, unresolved
imports, entry patterns with zero matches, project scopes that appear much
broader than entry coverage, and generated ignore patterns that also match
checked source files. When unresolved imports are present, JSON doctor output
includes sampled resolver diagnostics with matched aliases, expanded targets,
candidate files, and whether each candidate exists in the project.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the repository layout, architecture,
build graph, benchmarks, release artifacts, and local validation commands.
