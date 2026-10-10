#!/usr/bin/env bash
set -euo pipefail
source .buildkite/setup.sh

case "${1:-}" in
  native)
    bazel test //... --build_tests_only --test_tag_filters=-functional_test
    ;;
  functional)
    bazel test //... --build_tests_only --test_tag_filters=functional_test
    ;;
  bcr)
    install_node
    set -euo pipefail

    version="$(jq -r '.versions[-1]' bcr/modules/codescythe/metadata.json)"
    module="bcr/modules/codescythe/$version"
    cmp "$module/MODULE.bazel" "$module/overlay/MODULE.bazel"

    node -e '
      const fs = require("node:fs");
      const crypto = require("node:crypto");
      const root = process.argv[1];
      const source = JSON.parse(fs.readFileSync(`${root}/source.json`));
      if (!source.url.includes("/releases/download/")) {
        throw new Error(`BCR source URL is not a stable release asset: ${source.url}`);
      }
      for (const [name, expected] of Object.entries(source.overlay)) {
        const content = fs.readFileSync(`${root}/overlay/${name}`);
        const actual = `sha256-${crypto.createHash("sha256").update(content).digest("base64")}`;
        if (actual !== expected) throw new Error(`${name}: ${actual} !== ${expected}`);
      }
    ' "$module"

    consumer="$RUNNER_TEMP/codescythe-bcr"
    mkdir -p "$consumer"
    printf 'module(name = "codescythe_bcr_validation")\nbazel_dep(name = "codescythe", version = "%s")\n' \
      "$version" > "$consumer/MODULE.bazel"
    touch "$consumer/BUILD.bazel"
    cd "$consumer"

    bazel run \
      --lockfile_mode=off \
      --registry="file://$GITHUB_WORKSPACE/bcr" \
      --registry=https://bcr.bazel.build \
      @codescythe//:codescythe -- --version
    ;;
  *) echo 'Unknown CI suite' >&2; exit 1 ;;
esac
