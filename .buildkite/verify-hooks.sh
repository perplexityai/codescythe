#!/usr/bin/env bash
set -euo pipefail
source .buildkite/setup.sh
install_dependencies

work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT
if [[ "${BUILDKITE_PULL_REQUEST:-false}" != false ]]; then
  [[ "$BUILDKITE_PULL_REQUEST" =~ ^[1-9][0-9]*$ ]] || exit 1
  curl --fail --silent --show-error --location --retry 3 \
    "https://api.github.com/repos/perplexityai/codescythe/pulls/$BUILDKITE_PULL_REQUEST" \
    --output "$work_dir/pr.json"
  python3 - "$work_dir" <<'PY'
import json
import pathlib
import re
import sys
directory = pathlib.Path(sys.argv[1])
pr = json.loads((directory / 'pr.json').read_text())
for key in ('base', 'head'):
    sha = pr[key]['sha']
    if not re.fullmatch(r'[0-9a-f]{40}', sha):
        raise ValueError('Invalid pull request SHA')
    (directory / key).write_text(sha)
(directory / 'title').write_text(pr['title'] + '\n')
PY
  base=$(cat "$work_dir/base")
  head=$(cat "$work_dir/head")
  expected_head=${BUILDKITE_PULL_REQUEST_HEAD_COMMIT:-${BUILDKITE_COMMIT:?}}
  if [[ "$head" != "$expected_head" ]] || ! git merge-base --is-ancestor "$head" HEAD; then
    echo 'Pull request head changed; build its current revision' >&2
    exit 1
  fi
  git fetch --no-tags origin "$base"
  if [[ "$(git rev-parse --is-shallow-repository)" == true ]]; then
    git fetch --unshallow origin
  fi
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
