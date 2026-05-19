use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::Value;

#[test]
fn cli_reports_release_version() {
    let output = Command::new(runfile("crates/codescythe_cli/codescythe"))
        .arg("--version")
        .output()
        .expect("failed to run codescythe CLI");

    assert!(output.status.success(), "{}", output_text(&output));
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "codescythe 0.4.3"
    );
}

#[test]
fn cli_resolves_oxc_resolution_fixture() {
    let output = Command::new(runfile("crates/codescythe_cli/codescythe"))
        .args([
            "-C",
            path_arg(&runfile("tests/fixtures/oxc-resolution")),
            "--json",
        ])
        .output()
        .expect("failed to run codescythe CLI");

    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let analysis: Value =
        serde_json::from_slice(&output.stdout).expect("CLI stdout should be JSON");
    assert_eq!(analysis["counters"]["unresolved"], 0);

    let files = analysis["issues"]["files"]
        .as_object()
        .expect("files should be an object");
    assert!(files.contains_key("app/dead.ts"));
    assert!(!files.contains_key("app/aliased.ts"));
    assert!(!files.contains_key("app/internal.ts"));
    assert!(!files.contains_key("app/extension.ts"));

    let exports = analysis["issues"]["exports"]
        .as_object()
        .expect("exports should be an object");
    assert!(exports["app/aliased.ts"]
        .as_object()
        .expect("app/aliased.ts exports should be an object")
        .contains_key("unusedAliased"));
    assert!(exports["app/internal.ts"]
        .as_object()
        .expect("app/internal.ts exports should be an object")
        .contains_key("unusedInternal"));
    assert!(exports["app/extension.ts"]
        .as_object()
        .expect("app/extension.ts exports should be an object")
        .contains_key("unusedExtension"));
}

fn runfile(relative: &str) -> PathBuf {
    let relative = Path::new(relative);
    let mut candidates = Vec::new();

    if let Ok(runfiles_dir) = env::var("RUNFILES_DIR") {
        push_workspace_candidates(&mut candidates, &PathBuf::from(runfiles_dir), relative);
    }

    if let Ok(test_srcdir) = env::var("TEST_SRCDIR") {
        push_workspace_candidates(&mut candidates, &PathBuf::from(test_srcdir), relative);
    }

    if let Ok(current_exe) = env::current_exe() {
        for ancestor in current_exe.ancestors() {
            push_workspace_candidates(&mut candidates, ancestor, relative);
        }
    }

    for candidate in &candidates {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    panic!(
        "failed to locate runfile {}; tried: {}",
        relative.display(),
        candidates
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn push_workspace_candidates(candidates: &mut Vec<PathBuf>, root: &Path, relative: &Path) {
    candidates.push(root.join(relative));
    for workspace in [
        env::var("TEST_WORKSPACE").unwrap_or_else(|_| "_main".to_string()),
        "_main".to_string(),
        "codescythe".to_string(),
    ] {
        candidates.push(root.join(workspace).join(relative));
    }
}

fn path_arg(path: &Path) -> &str {
    path.to_str().expect("test paths should be valid UTF-8")
}

fn output_text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
