load("@with_cfg.bzl", "with_cfg")

def _release_node(platform):
    return with_cfg(native.genrule).set(
        "compilation_mode",
        "opt",
    ).set(
        "platforms",
        [Label(platform)],
    ).build()

release_node_linux_x64, _release_node_linux_x64_internal = _release_node(
    "//crates/codescythe_napi/platforms:linux_x64_gnu",
)

release_node_linux_arm64, _release_node_linux_arm64_internal = _release_node(
    "//crates/codescythe_napi/platforms:linux_arm64_gnu",
)

release_node_darwin_arm64, _release_node_darwin_arm64_internal = _release_node(
    "//crates/codescythe_napi/platforms:darwin_arm64",
)
