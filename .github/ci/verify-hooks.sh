#!/usr/bin/env bash
set -euo pipefail
source .github/ci/setup.sh
install_dependencies
work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT
if [[ "$GITHUB_EVENT_NAME" == pull_request ]]; then
  python3 - "$work_dir" <<'EVENT'
import json, os, pathlib
root = pathlib.Path(__import__('sys').argv[1])
pr = json.load(open(os.environ['GITHUB_EVENT_PATH']))['pull_request']
for key in ('base', 'head'):
    (root / key).write_text(pr[key]['sha'])
(root / 'title').write_text(pr['title'] + '\n')
EVENT
  base=$(cat "$work_dir/base")
  head=$(cat "$work_dir/head")
  export PR_TITLE="$(cat "$work_dir/title")"
else
  base=$(git rev-parse HEAD^)
  head=$(git rev-parse HEAD)
fi
git diff --name-only --diff-filter=ACMR "$base...$head" > "$work_dir/changed-files"
while IFS= read -r file; do
  [[ ! -f "$file" ]] || git add -- "$file"
done < "$work_dir/changed-files"
pnpm lefthook run pre-commit --force
if [[ -n "$(git status --porcelain)" ]]; then
  echo 'Pre-commit hooks changed files; run hooks locally and commit changes.' >&2
  git diff
  exit 1
fi
