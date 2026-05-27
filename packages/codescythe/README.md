# codescythe

Public Node package and CLI shim for Codescythe. This package loads the
matching unscoped native N-API package for the current platform:

- `codescythe-darwin-arm64`
- `codescythe-linux-amd64`
- `codescythe-linux-arm64`

The package entrypoints are TypeScript files and require Node's
`--experimental-transform-types` support. The package declares
`node >=22.18.0`.

Documentation: https://perplexityai.github.io/codescythe/

The public API exposes `analyze(options)`, `fix(options)`, and
`doctor(options)`. Options mirror the Rust CLI: pass `cwd`, `config`,
`verbose`, `force`, or `explainExport` as needed. Results are parsed JavaScript
objects returned from the native binding's JSON output.

Pass `verbose: true` to `analyze` or `fix`, or use `--verbose` with the package
bin, to include the same analysis diagnostics exposed by the Rust CLI.
