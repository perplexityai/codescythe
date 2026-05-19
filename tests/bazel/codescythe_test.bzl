_TOOLCHAIN_TYPE = Label("//tests/bazel:toolchain_type")
_ENV_SEPARATOR = "||CODESCYTHE_TEST_SEP||"

def _codescythe_toolchain_impl(ctx):
    return [
        platform_common.ToolchainInfo(
            codescythe = ctx.attr.codescythe[DefaultInfo],
        ),
    ]

codescythe_toolchain = rule(
    implementation = _codescythe_toolchain_impl,
    attrs = {
        "codescythe": attr.label(
            executable = True,
            cfg = "exec",
            mandatory = True,
        ),
    },
)

def _codescythe_test_impl(ctx):
    codescythe = ctx.toolchains[_TOOLCHAIN_TYPE].codescythe
    expanded_args = [
        ctx.expand_location(arg, targets = ctx.attr.data)
        for arg in ctx.attr.args
    ]
    executable = ctx.actions.declare_file(ctx.label.name + "_runner")
    ctx.actions.symlink(
        output = executable,
        target_file = ctx.executable._runner,
        is_executable = True,
    )

    runfiles = ctx.runfiles(
        files = ctx.files.data,
    ).merge(ctx.attr._runner[DefaultInfo].default_runfiles)
    runfiles = runfiles.merge(codescythe.default_runfiles)

    return [
        DefaultInfo(
            executable = executable,
            runfiles = runfiles,
        ),
        testing.TestEnvironment(
            {
                "CODESCYTHE_TEST_ARGS": _ENV_SEPARATOR.join(expanded_args),
                "CODESCYTHE_TEST_EXPECTED_EXIT_CODE": str(ctx.attr.expected_exit_code),
                "CODESCYTHE_TEST_MUST_CONTAIN": _ENV_SEPARATOR.join(ctx.attr.must_contain),
                "CODESCYTHE_TEST_MUST_NOT_CONTAIN": _ENV_SEPARATOR.join(ctx.attr.must_not_contain),
                "CODESCYTHE_TEST_PACKAGE": ctx.label.package,
                "CODESCYTHE_TEST_TOOL": codescythe.files_to_run.executable.short_path,
            },
        ),
    ]

codescythe_test = rule(
    implementation = _codescythe_test_impl,
    attrs = {
        "data": attr.label_list(allow_files = True),
        "expected_exit_code": attr.int(default = 0),
        "must_contain": attr.string_list(),
        "must_not_contain": attr.string_list(),
        "_runner": attr.label(
            default = Label("//tests/bazel:codescythe_test_runner"),
            executable = True,
            cfg = "exec",
        ),
    },
    test = True,
    toolchains = [_TOOLCHAIN_TYPE],
)
