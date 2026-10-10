# Buildkite CI

Native Rust tests, non-PR functional tests, Linux AMD64 and macOS ARM64 BCR overlay validation; Lefthook pre-commit checks. BCR validation runs on every build instead of only matching paths.

```mermaid
flowchart TD
  E["PR / main / merge group / manual build"] --> P["Trusted bootstrap: no checkout; verify GitHub PR"]
  P --> F{"Untrusted fork author?"}
  F -->|Yes| A["Pipeline writer approves commit; check stays Pending"]
  F -->|No: same repo or verified Renovate| C["Checkout; pin verified PR merge SHA"]
  A --> C
  C --> U["Upload native Buildkite jobs"]
  U --> T["Repository test matrix"]
  U --> H["Hook checks"]
  T --> S["Required Buildkite status"]
  H --> S
  E --> ARM["Linux ARM64 suites stay on GitHub Actions"]
  R["Release / deployment triggers"] --> G["Retained GitHub publishing workflows"]
```

Each Bazel job restores its suite/version/platform cache, runs checks, then saves after success. Hosted queues: `oss` (Linux AMD64), `oss_darwin_arm64` (macOS ARM64). Queue `default` is not used. Tools are checksum-pinned; pnpm follows `packageManager` or npm uses `npm ci`.

Paste `bootstrap.yml` into pipeline settings; never load approval policy from a PR checkout. Forks require writer approval before checkout or hooks. Only GitHub-verified `renovate[bot]` (ID `29139614`, type `Bot`) bypasses. Each new commit needs approval; API errors and stale PR heads fail closed. Enable third-party forks and publish blocked builds as Pending. Enable workflow-authorized GitHub tokens for release asset uploads. PR tokens remain read-only; merge queues cannot request tokens. Keep pipeline/cluster secrets disabled.

Build main pushes, PR updates/reopens/base changes, and merge groups; enable `codescythe_cli_v*` tags through the branch filter. Preserve PR exclusions shown in `pipeline.yml`. Manual builds support former workflow-dispatch checks. Retain required GHA ARM64 checks. Replace migrated required GHA checks with `buildkite/codescythe` from the Buildkite app only after native checks pass.

Remaining GHA: Release Please, CLI/npm/crates publishing, BCR publishing, GitHub Pages deployment.

Linux ARM64 jobs stay on GHA per requested runner policy. No new queue provisioned.

## Releases

CLI release tags build three binaries and verify `cargo package` on BK. BK uploads staging assets with a commit/checksum manifest using a short-lived GitHub App token. GHA waits for that manifest, checks the exact tag commit and every checksum, then smoke-tests all three binaries before publishing their usual release asset names and BCR overlay. Linux ARM64 smoke tests stay on GHA.

Crates GHA compares every packaged file against the BK-verified `.crate`, then uses OIDC and `cargo publish --locked --no-verify`; packaging repeats, compilation does not. npm release builds and provenance stay on GHA. Manual GHA dry runs select a CLI release tag containing this workflow; they verify existing BK staging assets without publishing.

```mermaid
flowchart TD
  Tag[CLI release tag] --> BK[BK: cross-build binaries and verify Cargo package]
  BK --> Assets[GitHub release: staging assets, SHA, checksums]
  Assets --> CLI[GHA: verify checksums and native smoke tests]
  CLI --> Release[CLI release assets and BCR]
  Assets --> Crate[GHA: compare packaged contents, no compilation]
  Crate --> Publish[crates.io OIDC publish]
  NPM[npm release] --> GHBuild[GHA build and provenance publish]
```

Pipeline settings: branch filter `main gh-readonly-queue/* codescythe_cli_v*`, GitHub tag builds enabled, workflow-authorized GitHub access tokens enabled. Upload requires a release tag whose commit is already on main; PR builds only produce test artifacts.
