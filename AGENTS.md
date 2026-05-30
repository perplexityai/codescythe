# Repository Instructions

- Write repo-local Node tooling as TypeScript (`.ts`) and run it with
  `node --experimental-transform-types`.
- Do not add `.mjs` tooling scripts.
- Lefthook owns repo-local hook checks. Use `pnpm format` to run pre-commit
  checks over all files.
- Pull request titles must follow Conventional Commits, such as
  `feat: add query output` or `fix(cli): handle missing config`. To validate one
  locally, set `PR_TITLE` and run `pnpm lefthook run pre-commit --force`.
