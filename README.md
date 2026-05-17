# Codescythe

Codescythe is a focused TypeScript dead-code analyzer and remover inspired by
Knip, scoped to entry/project graph analysis and unused TypeScript exports/files.
It intentionally avoids Knip's framework plugin surface.

## Development

```sh
bazel test //...
bazel run //:gazelle
```

Build native N-API artifacts:

```sh
bazel build //crates/codescythe_napi:release_nodes
```

The npm package loader lives in `npm/codescythe`, with native optional
dependency packages for:

- `npm/codescythe-linux-x64-gnu`
- `npm/codescythe-linux-arm64-gnu`
- `npm/codescythe-darwin-arm64`
