use std::{
    collections::BTreeSet,
    env,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
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
        "codescythe 0.4.7"
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

#[test]
fn cli_tracks_tests_as_leaf_files_and_fixes_removed_code_tests() {
    let cli = runfile("crates/codescythe_cli/codescythe");
    let fixture = runfile("tests/fixtures/test-file-usage");

    let output = Command::new(&cli)
        .args(["-C", path_arg(&fixture), "--json"])
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
    assert_json_snapshot(
        "test-file-usage analysis",
        &output.stdout,
        &runfile("tests/fixtures/test-file-usage/analysis.snapshot.json"),
    );

    let files = object_keys(&analysis["issues"]["files"]);
    assert!(files.contains("src/dead.ts"));
    assert!(files.contains("src/dead.test.ts"));
    assert!(files.contains("src/dead-wrapper.test.ts"));
    assert!(files.contains("src/namespace.test.ts"));
    assert!(files.contains("src/types.test.ts"));
    assert!(!files.contains("src/live.ts"));
    assert!(!files.contains("src/live.test.ts"));
    assert!(!files.contains("src/module.ts"));
    assert!(!files.contains("src/module.spec.ts"));
    assert!(!files.contains("src/namespace.ts"));
    assert!(!files.contains("src/types.ts"));

    let exports = analysis["issues"]["exports"]
        .as_object()
        .expect("exports should be an object");
    assert!(!exports.contains_key("src/module.ts"));
    assert!(exports["src/namespace.ts"]
        .as_object()
        .expect("src/namespace.ts exports should be an object")
        .contains_key("onlyForNamespaceTest"));
    assert!(exports["src/types.ts"]
        .as_object()
        .expect("src/types.ts exports should be an object")
        .contains_key("OnlyForTypeTest"));

    let writable_fixture = copy_fixture_to_temp(&fixture, "test-file-usage");
    let fix_output = Command::new(&cli)
        .args(["-C", path_arg(&writable_fixture), "--fix", "--json"])
        .output()
        .expect("failed to run codescythe CLI with --fix");

    assert_eq!(
        fix_output.status.code(),
        Some(1),
        "{}",
        output_text(&fix_output)
    );
    assert!(
        fix_output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&fix_output.stderr)
    );

    let fix_result: Value =
        serde_json::from_slice(&fix_output.stdout).expect("fix stdout should be JSON");
    assert_json_snapshot(
        "test-file-usage fix",
        &fix_output.stdout,
        &runfile("tests/fixtures/test-file-usage/fix.snapshot.json"),
    );

    assert_eq!(
        string_set(&fix_result["removedFiles"]),
        BTreeSet::from([
            "src/dead-wrapper.test.ts".to_string(),
            "src/dead.test.ts".to_string(),
            "src/dead.ts".to_string(),
            "src/namespace.test.ts".to_string(),
            "src/types.test.ts".to_string(),
        ])
    );
    assert_eq!(
        string_set(&fix_result["changedFiles"]),
        BTreeSet::from([
            "src/namespace.ts".to_string(),
            "src/types.ts".to_string(),
        ])
    );
    assert_eq!(fix_result["removedExports"], 2);

    assert!(!writable_fixture.join("src/dead.ts").exists());
    assert!(!writable_fixture.join("src/dead.test.ts").exists());
    assert!(!writable_fixture.join("src/dead-wrapper.test.ts").exists());
    assert!(writable_fixture.join("src/module.spec.ts").exists());
    assert!(!writable_fixture.join("src/namespace.test.ts").exists());
    assert!(!writable_fixture.join("src/types.test.ts").exists());
    assert!(writable_fixture.join("src/live.test.ts").exists());
    assert_eq!(
        fs::read_to_string(writable_fixture.join("src/module.ts")).unwrap(),
        "export const used = 1;\nexport const onlyForTest = 2;\n"
    );
    assert_eq!(
        fs::read_to_string(writable_fixture.join("src/namespace.ts")).unwrap(),
        "export const usedNamespace = 1;\n"
    );
    assert_eq!(
        fs::read_to_string(writable_fixture.join("src/types.ts")).unwrap(),
        "export type UsedType = { value: number };\n"
    );

    fs::remove_dir_all(&writable_fixture).unwrap();
}

#[test]
fn cli_fix_text_reports_unresolved_imports_before_summary() {
    let cli = runfile("crates/codescythe_cli/codescythe");
    let fixture = write_fixture_to_temp(
        "fix-unresolved",
        &[
            (
                "codescythe.json",
                r#"{
  "entry": "src/main.ts",
  "project": "src/**/*.ts"
}
"#,
            ),
            ("src/main.ts", "import './missing';\nconsole.log('entry');\n"),
        ],
    );

    let output = Command::new(&cli)
        .args(["-C", path_arg(&fixture), "--fix"])
        .output()
        .expect("failed to run codescythe CLI with --fix");

    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_text_snapshot(
        "fix unresolved stdout",
        &output.stdout,
        &runfile("tests/fixtures/cli-fix-output/unresolved.stdout"),
    );

    fs::remove_dir_all(&fixture).unwrap();
}

#[test]
fn cli_fix_text_output_for_clean_projects_stays_summary_only() {
    let cli = runfile("crates/codescythe_cli/codescythe");
    let fixture = write_fixture_to_temp(
        "fix-clean",
        &[
            (
                "codescythe.json",
                r#"{
  "entry": "src/main.ts",
  "project": "src/**/*.ts"
}
"#,
            ),
            ("src/main.ts", "const value = 1;\nconsole.log(value);\n"),
        ],
    );

    let output = Command::new(&cli)
        .args(["-C", path_arg(&fixture), "--fix"])
        .output()
        .expect("failed to run codescythe CLI with --fix");

    assert_eq!(output.status.code(), Some(0), "{}", output_text(&output));
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_text_snapshot(
        "fix clean stdout",
        &output.stdout,
        &runfile("tests/fixtures/cli-fix-output/clean.stdout"),
    );

    fs::remove_dir_all(&fixture).unwrap();
}

#[test]
fn cli_verbose_json_includes_diagnostics_snapshot() {
    let cli = runfile("crates/codescythe_cli/codescythe");
    let fixture = write_verbose_fixture_to_temp("verbose-json");

    let output = Command::new(&cli)
        .args(["-C", path_arg(&fixture), "--verbose", "--json"])
        .output()
        .expect("failed to run codescythe CLI with --verbose --json");

    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_verbose_json_snapshot(
        "verbose analysis json",
        &output.stdout,
        &fixture,
        &runfile("tests/fixtures/cli-verbose-output/analysis.snapshot.json"),
    );

    fs::remove_dir_all(&fixture).unwrap();
}

#[test]
fn cli_verbose_text_prints_diagnostics_to_stderr_snapshot() {
    let cli = runfile("crates/codescythe_cli/codescythe");
    let fixture = write_verbose_fixture_to_temp("verbose-text");

    let output = Command::new(&cli)
        .args(["-C", path_arg(&fixture), "--verbose"])
        .output()
        .expect("failed to run codescythe CLI with --verbose");

    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Unused files"),
        "{}",
        output_text(&output)
    );
    assert_text_snapshot(
        "verbose diagnostics stderr",
        normalize_verbose_text(&output.stderr, &fixture).as_bytes(),
        &runfile("tests/fixtures/cli-verbose-output/diagnostics.stderr"),
    );

    fs::remove_dir_all(&fixture).unwrap();
}

#[test]
fn cli_fix_verbose_json_includes_fix_plan_snapshot() {
    let cli = runfile("crates/codescythe_cli/codescythe");
    let fixture = write_verbose_fixture_to_temp("verbose-fix-json");

    let output = Command::new(&cli)
        .args(["-C", path_arg(&fixture), "--fix", "--verbose", "--json"])
        .output()
        .expect("failed to run codescythe CLI with --fix --verbose --json");

    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_verbose_json_snapshot(
        "verbose fix json",
        &output.stdout,
        &fixture,
        &runfile("tests/fixtures/cli-verbose-output/fix.snapshot.json"),
    );

    fs::remove_dir_all(&fixture).unwrap();
}

#[test]
fn cli_fix_verbose_text_prints_fix_plan_to_stderr_snapshot() {
    let cli = runfile("crates/codescythe_cli/codescythe");
    let fixture = write_verbose_fixture_to_temp("verbose-fix-text");

    let output = Command::new(&cli)
        .args(["-C", path_arg(&fixture), "--fix", "--verbose"])
        .output()
        .expect("failed to run codescythe CLI with --fix --verbose");

    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Removed 1 unused exports"),
        "{}",
        output_text(&output)
    );
    assert_text_snapshot(
        "verbose fix diagnostics stderr",
        normalize_verbose_text(&output.stderr, &fixture).as_bytes(),
        &runfile("tests/fixtures/cli-verbose-output/fix.diagnostics.stderr"),
    );

    fs::remove_dir_all(&fixture).unwrap();
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

fn object_keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .expect("value should be an object")
        .keys()
        .cloned()
        .collect()
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("value should be an array")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("array value should be a string")
                .to_string()
        })
        .collect()
}

fn assert_json_snapshot(name: &str, actual: &[u8], expected_path: &Path) {
    let actual = normalize_json(actual);
    let expected = fs::read_to_string(expected_path)
        .unwrap_or_else(|error| panic!("failed to read {name} snapshot: {error}"));
    assert_eq!(
        actual,
        expected,
        "{name} snapshot changed; expected snapshot at {}",
        expected_path.display()
    );
}

fn assert_text_snapshot(name: &str, actual: &[u8], expected_path: &Path) {
    let actual = String::from_utf8_lossy(actual);
    let expected = fs::read_to_string(expected_path)
        .unwrap_or_else(|error| panic!("failed to read {name} snapshot: {error}"));
    assert_eq!(
        actual,
        expected,
        "{name} snapshot changed; expected snapshot at {}",
        expected_path.display()
    );
}

fn assert_verbose_json_snapshot(name: &str, actual: &[u8], fixture: &Path, expected_path: &Path) {
    let actual = normalize_verbose_json(actual, fixture);
    let expected = fs::read_to_string(expected_path)
        .unwrap_or_else(|error| panic!("failed to read {name} snapshot: {error}"));
    assert_eq!(
        actual,
        expected,
        "{name} snapshot changed; expected snapshot at {}",
        expected_path.display()
    );
}

fn normalize_json(source: &[u8]) -> String {
    let value = serde_json::from_slice::<Value>(source).expect("source should be JSON");
    format!(
        "{}\n",
        serde_json::to_string_pretty(&value).expect("value should serialize")
    )
}

fn normalize_verbose_json(source: &[u8], fixture: &Path) -> String {
    let mut value = serde_json::from_slice::<Value>(source).expect("source should be JSON");
    for diagnostics_path in ["/diagnostics", "/analysis/diagnostics"] {
        if let Some(runtime) = value
            .pointer_mut(&format!("{diagnostics_path}/runtime"))
            .and_then(Value::as_object_mut)
        {
            runtime.insert("processCwd".to_string(), Value::String("<cwd>".to_string()));
            runtime.insert(
                "resolvedDirectory".to_string(),
                Value::String("<fixture>".to_string()),
            );
            if let Some(config_source) = runtime
                .get_mut("configSource")
                .and_then(Value::as_object_mut)
            {
                config_source.insert(
                    "path".to_string(),
                    Value::String("<fixture>/codescythe.json".to_string()),
                );
            }
        }
        if let Some(package_imports) = value
            .pointer_mut(&format!(
                "{diagnostics_path}/config/aliases/packageJsonImports"
            ))
            .and_then(Value::as_object_mut)
        {
            package_imports.insert(
                "path".to_string(),
                Value::String("<fixture>/package.json".to_string()),
            );
        }
    }
    let fixture = fixture.to_string_lossy();
    let rendered = serde_json::to_string_pretty(&value).expect("value should serialize");
    format!("{}\n", rendered.replace(fixture.as_ref(), "<fixture>"))
}

fn normalize_verbose_text(source: &[u8], fixture: &Path) -> String {
    String::from_utf8_lossy(source)
        .replace(&env::current_dir().unwrap().to_string_lossy().to_string(), "<cwd>")
        .replace(&fixture.to_string_lossy().to_string(), "<fixture>")
}

fn write_fixture_to_temp(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after UNIX_EPOCH")
        .as_nanos();
    let target = env::temp_dir().join(format!(
        "codescythe-e2e-{name}-{}-{nanos}",
        std::process::id()
    ));
    for (relative, contents) in files {
        let path = target.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }
    target
}

fn write_verbose_fixture_to_temp(name: &str) -> PathBuf {
    write_fixture_to_temp(
        name,
        &[
            (
                "codescythe.json",
                r##"{
  "entry": ["src/main.ts", "src/missing.ts", "src/entries/*.ts"],
  "project": ["src/**/*.ts", "ignored/**/*.ts"],
  "ignore": ["ignored/**"],
  "testFilePatterns": "src/**/*.test.ts",
  "aliases": {
    "#alias/*": "./src/*.ts"
  },
  "unresolvedImports": {
    "ignore": ["#virtual/**"]
  }
}
"##,
            ),
            (
                "package.json",
                r##"{
  "imports": {
    "#pkg/*": "./src/*.ts"
  }
}
"##,
            ),
            (".gitignore", "src/gitignored.ts\n"),
            (
                "src/main.ts",
                "import { used } from '#pkg/used';\nimport './dead.test';\nconsole.log(used);\n",
            ),
            (
                "src/used.ts",
                "export const used = 1;\nexport const unused = 2;\n",
            ),
            ("src/dead.ts", "export const dead = 1;\n"),
            (
                "src/dead.test.ts",
                "import { dead } from './dead';\nconsole.log(dead);\n",
            ),
            ("src/entries/extra.ts", "console.log('extra entry');\n"),
            ("src/gitignored.ts", "export const gitignored = 1;\n"),
            ("ignored/configIgnored.ts", "export const ignored = 1;\n"),
            ("README.md", "not source\n"),
        ],
    )
}

fn copy_fixture_to_temp(source: &Path, name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after UNIX_EPOCH")
        .as_nanos();
    let target = env::temp_dir().join(format!(
        "codescythe-e2e-{name}-{}-{nanos}",
        std::process::id()
    ));
    copy_dir(source, &target);
    target
}

fn copy_dir(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let metadata = fs::metadata(entry.path()).unwrap();
        let output = target.join(entry.file_name());
        if metadata.is_dir() {
            copy_dir(&entry.path(), &output);
        } else if metadata.is_file() {
            fs::copy(entry.path(), output).unwrap();
        }
    }
}
