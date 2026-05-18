CodescytheSourcesInfo = provider(
    doc = "Source files collected for a Codescythe runfiles test.",
    fields = {
        "sources": "depset of source files",
    },
)

def _source_group_impl(ctx):
    return [
        DefaultInfo(
            files = depset(
                ctx.files.srcs,
                transitive = [dep[DefaultInfo].files for dep in ctx.attr.deps],
            ),
        ),
    ]

source_group = rule(
    implementation = _source_group_impl,
    attrs = {
        "deps": attr.label_list(),
        "srcs": attr.label_list(allow_files = True),
    },
)

def _codescythe_sources_aspect_impl(target, ctx):
    transitive = []
    if hasattr(ctx.rule.attr, "deps"):
        transitive.extend([
            dep[CodescytheSourcesInfo].sources
            for dep in ctx.rule.attr.deps
            if CodescytheSourcesInfo in dep
        ])

    direct = []
    if hasattr(ctx.rule.attr, "srcs"):
        for src in ctx.rule.attr.srcs:
            direct.extend(src.files.to_list())

    return [
        CodescytheSourcesInfo(
            sources = depset(direct, transitive = transitive),
        ),
    ]

_codescythe_sources_aspect = aspect(
    implementation = _codescythe_sources_aspect_impl,
    attr_aspects = ["deps"],
)

def _codescythe_test_impl(ctx):
    source_depsets = [
        target[CodescytheSourcesInfo].sources
        for target in ctx.attr.targets
        if CodescytheSourcesInfo in target
    ]
    script = ctx.actions.declare_file(ctx.label.name + ".sh")
    ctx.actions.write(
        output = script,
        is_executable = True,
        content = _test_script(
            codescythe = ctx.executable._codescythe.short_path,
            config = ctx.file.config.short_path,
            expected_exit_code = ctx.attr.expected_exit_code,
            must_contain = ctx.attr.must_contain,
            must_not_contain = ctx.attr.must_not_contain,
        ),
    )

    runfiles = ctx.runfiles(
        files = [ctx.executable._codescythe, ctx.file.config] + ctx.files.data,
        transitive_files = depset(transitive = source_depsets),
    ).merge(ctx.attr._codescythe[DefaultInfo].default_runfiles)

    return [DefaultInfo(executable = script, runfiles = runfiles)]

codescythe_test = rule(
    implementation = _codescythe_test_impl,
    attrs = {
        "config": attr.label(allow_single_file = True, mandatory = True),
        "data": attr.label_list(allow_files = True),
        "expected_exit_code": attr.int(default = 0),
        "must_contain": attr.string_list(),
        "must_not_contain": attr.string_list(),
        "targets": attr.label_list(
            aspects = [_codescythe_sources_aspect],
            mandatory = True,
        ),
        "_codescythe": attr.label(
            default = Label("//crates/codescythe_cli:codescythe"),
            executable = True,
            cfg = "exec",
        ),
    },
    test = True,
)

def _test_script(codescythe, config, expected_exit_code, must_contain, must_not_contain):
    return """#!/usr/bin/env bash
set -euo pipefail

runfile() {{
  local path="$1"
  if [[ -n "${{RUNFILES_DIR:-}}" && -e "${{RUNFILES_DIR}}/${{path}}" ]]; then
    printf '%s\\n' "${{RUNFILES_DIR}}/${{path}}"
    return
  fi
  if [[ -n "${{RUNFILES_DIR:-}}" ]]; then
    for workspace in "${{TEST_WORKSPACE:-_main}}" _main codescythe; do
      if [[ -e "${{RUNFILES_DIR}}/${{workspace}}/${{path}}" ]]; then
        printf '%s\\n' "${{RUNFILES_DIR}}/${{workspace}}/${{path}}"
        return
      fi
    done
  fi
  if [[ -n "${{TEST_SRCDIR:-}}" ]]; then
    for workspace in "${{TEST_WORKSPACE:-_main}}" _main codescythe; do
      if [[ -e "${{TEST_SRCDIR}}/${{workspace}}/${{path}}" ]]; then
        printf '%s\\n' "${{TEST_SRCDIR}}/${{workspace}}/${{path}}"
        return
      fi
    done
  fi
  printf '%s\\n' "${{path}}"
}}

codescythe="$(runfile {codescythe})"
config="$(runfile {config})"
stdout="${{TEST_TMPDIR}}/codescythe.stdout.json"
stderr="${{TEST_TMPDIR}}/codescythe.stderr.txt"

set +e
"${{codescythe}}" --config "${{config}}" --json --compact-json >"${{stdout}}" 2>"${{stderr}}"
status="$?"
set -e

if [[ "${{status}}" -ne {expected_exit_code} ]]; then
  echo "expected exit code {expected_exit_code}, got ${{status}}" >&2
  cat "${{stdout}}" >&2
  cat "${{stderr}}" >&2
  exit 1
fi

if [[ -s "${{stderr}}" ]]; then
  cat "${{stderr}}" >&2
  exit 1
fi

must_contain=({must_contain})
for needle in "${{must_contain[@]}}"; do
  if ! grep -F -- "${{needle}}" "${{stdout}}" >/dev/null; then
    echo "expected Codescythe output to contain: ${{needle}}" >&2
    cat "${{stdout}}" >&2
    exit 1
  fi
done

must_not_contain=({must_not_contain})
for needle in "${{must_not_contain[@]}}"; do
  if grep -F -- "${{needle}}" "${{stdout}}" >/dev/null; then
    echo "expected Codescythe output not to contain: ${{needle}}" >&2
    cat "${{stdout}}" >&2
    exit 1
  fi
done
""".format(
        codescythe = _shell_quote(codescythe),
        config = _shell_quote(config),
        expected_exit_code = expected_exit_code,
        must_contain = " ".join([_shell_quote(value) for value in must_contain]),
        must_not_contain = " ".join([_shell_quote(value) for value in must_not_contain]),
    )

def _shell_quote(value):
    return "'" + value.replace("'", "'\\''") + "'"
