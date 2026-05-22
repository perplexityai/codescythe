# Benchmarks

`run.ts` uses Benchmark.js to time analyzer CLI runs against pinned real-world
source snapshots fetched through Bazel:

- `microsoft/vscode` at `9b7643f90393b9ad2c5d5cbbdad70fa928090009`.
- `grafana/grafana` at `7709dc39cf8ee2de85c38b8943b208adf8a3c47c`.
- `elastic/kibana` at `d706f62a04af1112db6b4dfef3c94955bdb98250`.
- `renovatebot/renovate` at `b42bb1dc25287ab0b2b328559674e442d3290da9`.

Run the default benchmark. It builds Codescythe, fetches all fixtures through
`MODULE.bazel`, and compares against the workspace's Knip dev dependency when
available.

```sh
pnpm benchmark
```

Useful options:

```sh
node --experimental-transform-types benchmarks/run.ts --fixture vscode --samples 7
node --experimental-transform-types benchmarks/run.ts --fixture grafana --samples 7
node --experimental-transform-types benchmarks/run.ts --fixture kibana --samples 7
node --experimental-transform-types benchmarks/run.ts --fixture renovate --samples 7
node --experimental-transform-types benchmarks/run.ts --skip-build --skip-knip
pnpm conformance:kibana
bazel run //benchmarks:vscode_conformance
bazel run //benchmarks:grafana_conformance
bazel run //benchmarks:renovate_conformance
CODESCYTHE_BIN=/tmp/codescythe KNIP_BIN=/tmp/knip pnpm benchmark
CODESCYTHE_PARSE_THREADS=4 pnpm benchmark
```

Codescythe is measured with `--json --directory <fixture>
--config <generated-config>`. Knip is measured only when available, with
reporting limited to file, value-export, and type-export issues so the
comparison stays close to Codescythe's scope. The generated config is shared by
Codescythe and Knip, except Codescythe disables its default test-file leaf
patterns for these whole-corpus benchmark configs. VS Code uses source-root
entry/project globs for `src`, `build`, and `extensions`. Grafana uses
source-root entry/project globs for `public`, `packages`, `scripts`, and the
root TypeScript config files. Kibana uses the root `knip.jsonc` introduced in
`elastic/kibana#270237`; Codescythe uses the equivalent source graph (`src`,
`x-pack`, `packages`, `examples`, and `plugins` with `ts`, `tsx`, `js`, and
`jsx` files), while Knip runs with the imported
`benchmarks/kibana_pr270237_knip.jsonc`. Renovate uses the source-side CLI,
config-validator, `tsdown` package entries, TypeScript tooling scripts,
JavaScript/MJS tooling entrypoints, and test entrypoints instead of generated
`dist/` package bins. The repo installs Knip as a dev dependency; set `KNIP_BIN`
to compare against a different Knip binary.

The Kibana source snapshot expects `@kbn/tsconfig-base` to exist in
`node_modules`, but the Bazel-fetched fixture only contains source files. The
benchmark harness writes a tsconfig shim into the temporary fixture checkout
that matches Kibana's `@kbn/tsconfig-base` package by extending the root
`tsconfig.base.json` before running either tool.

Codescythe discovers the full project file set up front, then parses files in
parallel graph-frontier batches from the configured entries. The source-root
fixture configs still measure whole-corpus parsing rather than the lazy path's
best case. Set `CODESCYTHE_PARSE_THREADS` to tune parse parallelism;
`RAYON_NUM_THREADS` is respected when the Codescythe-specific variable is
unset.

## Current Numbers

Local run on May 22, 2026 with the checked-in fixture configs. The Kibana rows
use the imported `elastic/kibana#270237` Knip config and a one-run smoke sample.

```sh
bazel build -c opt //crates/codescythe_cli:codescythe
node --experimental-transform-types benchmarks/run.ts --samples 3 --warmups 1 --codescythe-bin bazel-bin/crates/codescythe_cli/codescythe --skip-build
```

```text
fixture    tool        mean       rme        samples  ops/sec
---------  ----------  ---------  ---------  -------  -------
vscode     codescythe  1111.9ms   +/-1.36%   5        0.90
vscode     knip        4223.1ms   +/-4.89%   3        0.24
grafana    codescythe  833.2ms    +/-4.11%   5        1.20
grafana    knip        9513.4ms   +/-56.75%  3        0.11
kibana     codescythe  13609.0ms  +/-0.00%   1        0.07
kibana     knip        43044.0ms   +/-0.00%   1        0.02
renovate   codescythe  154.5ms    +/-4.79%   18       6.47
renovate   knip        900.5ms    +/-6.09%   5        1.11
```

## Vendored Conformance

The conformance snapshots copy each fixture to a temporary directory, inject
synthetic unused TypeScript files and reachable modules with unused exports, run
the configured graph through Codescythe and Knip, then run a fuzz-only
`codescythe --fix` pass followed by a post-fix Codescythe analysis. Synthetic
unused files stay outside the entry set, while synthetic export modules are
imported by a real fixture entry so only the fuzzed exports are eligible for
`--fix`. Knip framework plugins are disabled so the file-level result checks the
configured TypeScript graph:

- Every file Knip reports unused must also be reported unused by Codescythe.
- Every synthetic fuzz file must be reported unused by both tools.
- Every synthetic unused export must be reported unused by Codescythe, while
  the synthetic export used by a reachable fixture entrypoint must stay clean.
- `codescythe --fix` must remove the synthetic unused files, remove the
  synthetic unused exports from source, keep the synthetic used exports, and
  leave no synthetic file or export issue in the post-fix analysis.

The pinned real-repo fixtures are also covered by Bazel functional tests:
`//benchmarks:vscode_fixture_test`, `//benchmarks:grafana_fixture_test`,
`//benchmarks:kibana_fixture_test`, and `//benchmarks:renovate_fixture_test`.
Each fixture test runs the benchmark harness once against the Bazel-fetched
source tree using the checked-in entry/project config. The snapshot checks live
at `//benchmarks:vscode_conformance`, `//benchmarks:grafana_conformance`,
`//benchmarks:kibana_conformance`, and `//benchmarks:renovate_conformance`.
The functional test targets are tagged `functional_test` so CI can keep the
normal test lane on pull requests with
`bazel test //... --build_tests_only --test_tag_filters=-functional_test` and
run the slower functional lane from the main-only `test functional` workflow
with `bazel test //... --build_tests_only --test_tag_filters=functional_test`.
The conformance targets are `write_source_file` snapshot checks; run
`bazel run //benchmarks:<fixture>_conformance` to refresh checked-in JSON after
reviewing an intentional conformance change. Use `--skip-build`,
`--codescythe-bin`, `--knip-bin`, `--fuzz-files`, `--fuzz-exports`, `--seed`,
and `--snapshot-output` to control local script runs.
