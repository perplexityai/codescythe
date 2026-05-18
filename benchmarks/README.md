# Benchmarks

`run.ts` uses Benchmark.js to time analyzer CLI runs against pinned real-world
source snapshots fetched through Bazel:

- `microsoft/vscode` at `9b7643f90393b9ad2c5d5cbbdad70fa928090009`.
- `grafana/grafana` at `7709dc39cf8ee2de85c38b8943b208adf8a3c47c`.
- `elastic/kibana` at `d706f62a04af1112db6b4dfef3c94955bdb98250`.

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
node --experimental-transform-types benchmarks/run.ts --skip-build --skip-knip
pnpm conformance:kibana
CODESCYTHE_BIN=/tmp/codescythe KNIP_BIN=/tmp/knip pnpm benchmark
CODESCYTHE_PARSE_THREADS=4 pnpm benchmark
```

Codescythe is measured with `--json --compact-json --directory <fixture>
--config <generated-config>`. Knip is measured only when available, with
reporting limited to file, value-export, and type-export issues so the
comparison stays close to Codescythe's scope. The generated config is shared by
Codescythe and Knip. By default it treats all TypeScript-family source files as
both `entry` and `project`; fixtures can override `entry` when a realistic
entry graph is more useful. The Kibana benchmark uses explicit core/security
entrypoints against the full project set. The repo installs Knip as a dev
dependency; set `KNIP_BIN` to compare against a different Knip binary.

The Kibana source snapshot expects `@kbn/tsconfig-base` to exist in
`node_modules`, but the Bazel-fetched fixture only contains source files. The
benchmark harness writes a tsconfig shim into the temporary fixture checkout
that matches Kibana's `@kbn/tsconfig-base` package by extending the root
`tsconfig.base.json` before running either tool.

Codescythe discovers the full project file set up front, then parses files in
parallel graph-frontier batches from the configured entries. Because the default
benchmark config makes every TypeScript-family file an entry, it still measures
whole-corpus parsing rather than the lazy path's best case. Set
`CODESCYTHE_PARSE_THREADS` to tune parse parallelism; `RAYON_NUM_THREADS` is
respected when the Codescythe-specific variable is unset.

## Current Kibana Numbers

Local run on May 18, 2026:

```sh
node --experimental-transform-types benchmarks/run.ts --fixture kibana --samples 3 --warmups 1 --skip-build
```

```text
tool        mean       rme        samples  ops/sec
----------  ---------  ---------  -------  -------
codescythe  2170.2ms   +/-11.95%  4        0.46
knip        48742.7ms  +/-30.00%  3        0.02
```

The matching conformance run with 16 injected unused files reported
82,969 unused files for Codescythe and 82,965 for Knip out of 86,072
benchmarked files. Codescythe covered every Knip unused file, both tools found
all synthetic files, and the 4 Codescythe-only files were imported only by other
unused files.

## Kibana Conformance

`pnpm conformance:kibana` copies the Kibana fixture to a temporary directory,
injects synthetic unused TypeScript files, then runs a shared core-graph config
through Codescythe and Knip. The comparison disables Knip framework plugins so
the file-level result checks the configured TypeScript graph:

- Every file Knip reports unused must also be reported unused by Codescythe.
- Every synthetic fuzz file must be reported unused by both tools.
- Codescythe-only unused files are allowed only when every importer found in
  the project graph is also unused. This preserves Codescythe's more complete
  dead-subgraph reporting while guarding against reachable false positives.

The conformance script is also a Bazel test at
`//benchmarks:kibana_conformance_test`. It is tagged `functional_test` so CI can
run the normal test lane with
`bazel test //... --build_tests_only --test_tag_filters=-functional_test` and
the slower Kibana lane separately with
`bazel test //... --build_tests_only --test_tag_filters=functional_test`. Use
`--skip-build`, `--codescythe-bin`, `--knip-bin`, `--fuzz-files`, and `--seed`
to control local runs.
