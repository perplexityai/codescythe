# codescythe

Node bindings for Codescythe. This package loads the matching native N-API
package for Linux x64 GNU, Linux arm64 GNU, or Darwin arm64.

Pass `verbose: true` to `analyze` or `fix`, or use `--verbose` with the package
bin, to include the same analysis diagnostics exposed by the Rust CLI.
