load("@aspect_rules_js//js:defs.bzl", "js_test")

def fixture_functional_test(name, fixture, fixture_repo):
    js_test(
        name = name,
        args = [
            "--fixture",
            fixture,
            "--codescythe-bin",
            "$(rootpath //crates/codescythe_cli:codescythe)",
            "--fixture-package-json",
            "$(rootpath %s)" % (fixture_repo + "//:package_json"),
            "--once",
            "--skip-build",
            "--skip-knip",
        ],
        data = [
            "//:node_modules/benchmark",
            "//crates/codescythe_cli:codescythe",
            fixture_repo + "//:all_files",
            fixture_repo + "//:package_json",
        ],
        copy_data_to_bin = False,
        entry_point = "run.ts",
        node_options = ["--experimental-transform-types"],
        tags = ["functional_test"],
        timeout = "long",
    )
