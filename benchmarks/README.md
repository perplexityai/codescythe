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
CODESCYTHE_BIN=/tmp/codescythe KNIP_BIN=/tmp/knip pnpm benchmark
CODESCYTHE_PARSE_THREADS=4 pnpm benchmark
```

Codescythe is measured with `--json --compact-json --directory <fixture>
--config <generated-config>`. Knip is measured only when available, with
reporting limited to file, value-export, and type-export issues so the
comparison stays close to Codescythe's scope. The shared benchmark config treats
all TypeScript-family source files as both `entry` and `project`, which makes
this a stable whole-corpus parse/import/export benchmark instead of a
project-specific unused-file audit. The repo installs Knip as a dev
dependency; set `KNIP_BIN` to compare against a different Knip binary.

Codescythe parses files in parallel by default, capped at the machine's available
parallelism. Set `CODESCYTHE_PARSE_THREADS` to tune this for local filesystem
behavior; `RAYON_NUM_THREADS` is also respected when the Codescythe-specific
variable is unset.
