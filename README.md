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
bazel build //crates/codescythe_cli:release_binaries
bazel build //crates/codescythe_napi:release_nodes
bazel build //packages/codescythe:package //packages/codescythe-darwin-arm64:package //packages/codescythe-linux-amd64:package //packages/codescythe-linux-arm64:package
```

The npm package loader lives in `packages/codescythe`, with native optional
dependency packages for:

- `packages/codescythe-linux-amd64`
- `packages/codescythe-linux-arm64`
- `packages/codescythe-darwin-arm64`

Static CLI release binaries are built for:

- `codescythe-linux-amd64`
- `codescythe-linux-arm64`
- `codescythe-darwin-arm64`
