use super::*;
use std::{env, fs};
use walkdir::WalkDir;

#[test]
fn finds_unused_exports_and_files_in_knip_style_fixture() {
    let (_tempdir, cwd) = fixture_path("knip-export-basics");
    let config = crate::load_config(&cwd, None).unwrap();
    let analysis = analyze_path(&cwd, &config, AnalysisOptions::default()).unwrap();

    assert!(analysis.issues.files.contains_key("dangling.ts"));
    assert!(analysis.issues.exports["my-module.ts"].contains_key("unused"));
    assert!(analysis.issues.exports["my-module.ts"].contains_key("default"));
    assert!(analysis.issues.exports["my-namespace.ts"].contains_key("key"));
    assert!(analysis.issues.exports["types.ts"].contains_key("UnusedType"));
    assert!(!analysis.issues.exports.contains_key("index.ts"));
}

#[cfg(unix)]
#[test]
fn follows_runfiles_style_symlinked_source_directories() {
    let real = tempfile::tempdir().unwrap();
    let runfiles = tempfile::tempdir().unwrap();

    fs::write(
        real.path().join("codescythe.json"),
        r#"{
              "entry": ["app/index.ts"],
              "project": ["app/**/*.ts"]
            }"#,
    )
    .unwrap();
    fs::create_dir(real.path().join("app")).unwrap();
    fs::write(
        real.path().join("app/index.ts"),
        "import { used } from './used';\nconsole.log(used);\n",
    )
    .unwrap();
    fs::write(real.path().join("app/used.ts"), "export const used = 1;\n").unwrap();
    fs::write(real.path().join("app/dead.ts"), "export const dead = 1;\n").unwrap();

    std::os::unix::fs::symlink(
        real.path().join("codescythe.json"),
        runfiles.path().join("codescythe.json"),
    )
    .unwrap();
    std::os::unix::fs::symlink(real.path().join("app"), runfiles.path().join("app")).unwrap();

    let config = crate::load_config(runfiles.path(), None).unwrap();
    let analysis = analyze_path(runfiles.path(), &config, AnalysisOptions::default()).unwrap();

    assert_eq!(analysis.counters.total, 3);
    assert!(analysis.issues.files.contains_key("app/dead.ts"));
    assert!(!analysis.issues.files.contains_key("app/used.ts"));
}

#[cfg(unix)]
#[test]
fn prunes_configured_ignored_directories_before_following_links() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    fs::write(
        cwd.join("codescythe.json"),
        r#"{
              "entry": ["src/main.ts"],
              "project": ["src/**/*.ts"],
              "ignore": [".pnpm", ".pnpm/**"]
            }"#,
    )
    .unwrap();
    fs::create_dir(cwd.join("src")).unwrap();
    fs::write(cwd.join("src/main.ts"), "console.log('entry');\n").unwrap();
    fs::create_dir_all(cwd.join(".pnpm/store/v10/projects")).unwrap();
    std::os::unix::fs::symlink(
        cwd.join("missing-worktree"),
        cwd.join(".pnpm/store/v10/projects/stale"),
    )
    .unwrap();

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert_eq!(analysis.counters.total, 1);
    assert!(analysis.issues.files.is_empty());
}

#[test]
fn applies_gitignore_to_project_discovery_by_default() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r#"{
              "entry": "src/main.ts",
              "project": ["src/**/*.ts", "generated/**/*.ts"]
            }"#,
    );
    write_file(
        cwd,
        ".gitignore",
        "generated/\n*.dead.ts\n!src/local.dead.ts\n",
    );
    write_file(
        cwd,
        "src/main.ts",
        "import { used } from './used';\nconsole.log(used);\n",
    );
    write_file(cwd, "src/used.ts", "export const used = 1;\n");
    write_file(cwd, "src/local.dead.ts", "export const dead = 1;\n");
    write_file(cwd, "generated/client.ts", "export const client = 1;\n");

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert_eq!(analysis.counters.total, 3);
    assert!(analysis.issues.files.contains_key("src/local.dead.ts"));
    assert!(!analysis.issues.files.contains_key("generated/client.ts"));
}

#[test]
fn discovers_nested_gitignore_files_by_default() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r#"{
              "entry": "src/main.ts",
              "project": "src/**/*.ts"
            }"#,
    );
    write_file(cwd, "src/feature/.gitignore", "ignored.ts\n!kept.ts\n");
    write_file(
        cwd,
        "src/main.ts",
        "import { kept } from './feature/kept';\nconsole.log(kept);\n",
    );
    write_file(cwd, "src/feature/kept.ts", "export const kept = 1;\n");
    write_file(cwd, "src/feature/ignored.ts", "export const ignored = 1;\n");

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert_eq!(analysis.counters.total, 2);
    assert!(!analysis.issues.files.contains_key("src/feature/kept.ts"));
    assert!(!analysis.issues.files.contains_key("src/feature/ignored.ts"));
}

#[test]
fn follows_oxc_resolution_rules_for_project_imports() {
    let (_tempdir, cwd) = fixture_path("oxc-resolution");

    let config = crate::load_config(&cwd, None).unwrap();
    let analysis = analyze_path(&cwd, &config, AnalysisOptions::default()).unwrap();

    assert!(analysis.issues.unresolved.is_empty());
    assert!(analysis.issues.files.contains_key("app/dead.ts"));
    assert!(!analysis.issues.files.contains_key("app/aliased.ts"));
    assert!(!analysis.issues.files.contains_key("app/internal.ts"));
    assert!(!analysis.issues.files.contains_key("app/extension.ts"));
    assert!(!analysis.issues.exports["app/aliased.ts"].contains_key("aliased"));
    assert!(analysis.issues.exports["app/aliased.ts"].contains_key("unusedAliased"));
    assert!(!analysis.issues.exports["app/internal.ts"].contains_key("internal"));
    assert!(analysis.issues.exports["app/internal.ts"].contains_key("unusedInternal"));
    assert!(!analysis.issues.exports["app/extension.ts"].contains_key("extension"));
    assert!(analysis.issues.exports["app/extension.ts"].contains_key("unusedExtension"));
}

#[test]
fn reads_package_json_imports_by_default() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r#"{
              "entry": "src/main.ts",
              "project": "src/**/*.ts"
            }"#,
    );
    write_file(
        cwd,
        "package.json",
        r##"{
              "imports": {
                "#app/*": "./src/*.ts"
              }
            }"##,
    );
    write_file(
        cwd,
        "src/main.ts",
        "import { used } from '#app/used';\nconsole.log(used);\n",
    );
    write_file(cwd, "src/used.ts", "export const used = 1;\n");
    write_file(cwd, "src/unused.ts", "export const unused = 1;\n");

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert!(analysis.issues.unresolved.is_empty());
    assert!(!analysis.issues.files.contains_key("src/used.ts"));
    assert!(analysis.issues.files.contains_key("src/unused.ts"));
    assert!(
        !analysis
            .issues
            .exports
            .get("src/used.ts")
            .is_some_and(|exports| exports.contains_key("used"))
    );
}

#[test]
fn test_entries_do_not_keep_production_files_alive() {
    let analysis = analyze_inline_project_with_config(
        r#"{
              "entry": ["src/main.ts", "src/**/*.test.ts"],
              "project": "src/**/*.ts"
            }"#,
        &[
            ("src/main.ts", "console.log('entry');\n"),
            ("src/dead.ts", "export const dead = 1;\n"),
            (
                "src/dead.test.ts",
                "import { dead } from './dead';\nconsole.log(dead);\n",
            ),
        ],
    );

    assert_unused_file(&analysis, "src/dead.ts");
    assert_unused_file(&analysis, "src/dead.test.ts");
}

#[test]
fn test_imports_do_not_keep_exports_alive() {
    let analysis = analyze_inline_project_with_config(
        r#"{
              "entry": ["src/main.ts", "src/**/*.spec.ts"],
              "project": "src/**/*.ts",
              "testFilePatterns": "src/**/*.spec.ts"
            }"#,
        &[
            (
                "src/main.ts",
                "import { used } from './module';\nconsole.log(used);\n",
            ),
            (
                "src/module.ts",
                "export const used = 1;\nexport const onlyForTest = 2;\n",
            ),
            (
                "src/module.spec.ts",
                "import { onlyForTest } from './module';\nconsole.log(onlyForTest);\n",
            ),
        ],
    );

    assert_unused_export(&analysis, "src/module.ts", "onlyForTest");
    assert_unused_file(&analysis, "src/module.spec.ts");
}

#[test]
fn spec_entries_keep_exports_alive_by_default() {
    let analysis = analyze_inline_project_with_config(
        r#"{
              "entry": ["src/main.ts", "src/**/*.spec.ts"],
              "project": "src/**/*.ts"
            }"#,
        &[
            (
                "src/main.ts",
                "import { used } from './module';\nconsole.log(used);\n",
            ),
            (
                "src/module.ts",
                "export const used = 1;\nexport const onlyForSpec = 2;\n",
            ),
            (
                "src/module.spec.ts",
                "import { onlyForSpec } from './module';\nconsole.log(onlyForSpec);\n",
            ),
        ],
    );

    assert_no_unused_export(&analysis, "src/module.ts", "onlyForSpec");
    assert!(!analysis.issues.files.contains_key("src/module.spec.ts"));
}

#[test]
fn tests_for_live_code_are_kept_as_leaf_files() {
    let analysis = analyze_inline_project(&[
        (
            "src/entry.ts",
            "import { used } from './module';\nconsole.log(used);\n",
        ),
        ("src/module.ts", "export const used = 1;\n"),
        (
            "src/module.test.ts",
            "import { used } from './module';\nconsole.log(used);\n",
        ),
    ]);

    assert!(!analysis.issues.files.contains_key("src/module.test.ts"));
    assert_no_unused_export(&analysis, "src/module.ts", "used");
}

#[test]
fn default_test_file_patterns_mark_removed_file_tests_unused_without_test_entries() {
    let analysis = analyze_inline_project(&[
        ("src/entry.ts", "console.log('entry');\n"),
        ("src/dead.ts", "export const dead = 1;\n"),
        (
            "src/dead.test.ts",
            "import { dead } from './dead';\nconsole.log(dead);\n",
        ),
    ]);

    assert_unused_file(&analysis, "src/dead.ts");
    assert_unused_file(&analysis, "src/dead.test.ts");
}

#[test]
fn tests_tied_to_removed_tests_are_also_unused() {
    let analysis = analyze_inline_project(&[
        ("src/entry.ts", "console.log('entry');\n"),
        ("src/dead.ts", "export const dead = 1;\n"),
        (
            "src/dead.test.ts",
            "import { dead } from './dead';\nconsole.log(dead);\n",
        ),
        ("src/dead-wrapper.test.ts", "import './dead.test';\n"),
    ]);

    assert_unused_file(&analysis, "src/dead.ts");
    assert_unused_file(&analysis, "src/dead.test.ts");
    assert_unused_file(&analysis, "src/dead-wrapper.test.ts");
}

#[test]
fn namespace_usage_of_test_only_exports_marks_test_unused() {
    let analysis = analyze_inline_project(&[
        (
            "src/entry.ts",
            "import { used } from './module';\nconsole.log(used);\n",
        ),
        (
            "src/module.ts",
            "export const used = 1;\nexport const onlyForTest = 2;\n",
        ),
        (
            "src/module.test.ts",
            "import * as module from './module';\nconsole.log(module.onlyForTest);\n",
        ),
    ]);

    assert_unused_export(&analysis, "src/module.ts", "onlyForTest");
    assert_unused_file(&analysis, "src/module.test.ts");
}

#[test]
fn type_imports_in_tests_mark_test_only_types_unused() {
    let analysis = analyze_inline_project(&[
        (
            "src/entry.ts",
            "import type { UsedType } from './types';\nconst value: UsedType = { value: 1 };\nconsole.log(value);\n",
        ),
        (
            "src/types.ts",
            "export type UsedType = { value: number };\nexport type OnlyForTest = { value: number };\n",
        ),
        (
            "src/types.test.ts",
            "import type { OnlyForTest } from './types';\nconst value: OnlyForTest = { value: 1 };\nconsole.log(value);\n",
        ),
    ]);

    assert_no_unused_export(&analysis, "src/types.ts", "UsedType");
    assert_unused_export(&analysis, "src/types.ts", "OnlyForTest");
    assert_unused_file(&analysis, "src/types.test.ts");
}

#[test]
fn propagates_used_export_through_already_reached_barrel() {
    let analysis = analyze_inline_project(&[
        ("src/entry.ts", "import './form';\nimport './feature';\n"),
        (
            "src/feature.ts",
            "import { FormCheckbox } from './form';\nconsole.log(FormCheckbox);\n",
        ),
        (
            "src/form/index.ts",
            "export { FormCheckbox } from './FormCheckbox';\n",
        ),
        (
            "src/form/FormCheckbox.ts",
            "export const FormCheckbox = () => null;\n",
        ),
    ]);

    assert_no_unused_export(&analysis, "src/form/index.ts", "FormCheckbox");
    assert_no_unused_export(&analysis, "src/form/FormCheckbox.ts", "FormCheckbox");
}

#[test]
fn propagates_used_export_through_multi_hop_barrels() {
    let analysis = analyze_inline_project(&[
        ("src/entry.ts", "import './ui';\nimport './feature';\n"),
        (
            "src/feature.ts",
            "import { FormCheckbox } from './ui';\nconsole.log(FormCheckbox);\n",
        ),
        ("src/ui.ts", "export { FormCheckbox } from './form';\n"),
        (
            "src/form/index.ts",
            "export { FormCheckbox } from './FormCheckbox';\n",
        ),
        (
            "src/form/FormCheckbox.ts",
            "export const FormCheckbox = () => null;\n",
        ),
    ]);

    assert_no_unused_export(&analysis, "src/ui.ts", "FormCheckbox");
    assert_no_unused_export(&analysis, "src/form/index.ts", "FormCheckbox");
    assert_no_unused_export(&analysis, "src/form/FormCheckbox.ts", "FormCheckbox");
}

#[test]
fn propagates_used_export_through_reexport_alias() {
    let analysis = analyze_inline_project(&[
        ("src/entry.ts", "import './form';\nimport './feature';\n"),
        (
            "src/feature.ts",
            "import { Checkbox } from './form';\nconsole.log(Checkbox);\n",
        ),
        (
            "src/form/index.ts",
            "export { FormCheckbox as Checkbox } from './FormCheckbox';\n",
        ),
        (
            "src/form/FormCheckbox.ts",
            "export const FormCheckbox = () => null;\n",
        ),
    ]);

    assert_no_unused_export(&analysis, "src/form/index.ts", "Checkbox");
    assert_no_unused_export(&analysis, "src/form/FormCheckbox.ts", "FormCheckbox");
}

#[test]
fn reachable_reexport_stays_unused_without_named_usage() {
    let analysis = analyze_inline_project(&[
        ("src/entry.ts", "import './form';\n"),
        (
            "src/form/index.ts",
            "export { FormCheckbox } from './FormCheckbox';\n",
        ),
        (
            "src/form/FormCheckbox.ts",
            "export const FormCheckbox = () => null;\n",
        ),
    ]);

    assert_unused_export(&analysis, "src/form/index.ts", "FormCheckbox");
    assert_unused_export(&analysis, "src/form/FormCheckbox.ts", "FormCheckbox");
}

#[test]
fn unreachable_reexport_files_stay_unused() {
    let analysis = analyze_inline_project(&[
        ("src/entry.ts", "console.log('entry');\n"),
        (
            "src/form/index.ts",
            "export { FormCheckbox } from './FormCheckbox';\n",
        ),
        (
            "src/form/FormCheckbox.ts",
            "export const FormCheckbox = () => null;\n",
        ),
    ]);

    assert_unused_file(&analysis, "src/form/index.ts");
    assert_unused_file(&analysis, "src/form/FormCheckbox.ts");
}

#[test]
fn companion_test_imports_keep_test_helpers_reachable() {
    let analysis = analyze_inline_project_with_config(
        r#"{
              "entry": "src/entry.ts",
              "project": "src/**/*.ts",
              "testFilePatterns": "src/**/*.spec.ts"
            }"#,
        &[
            (
                "src/entry.ts",
                "import { formatPrice } from './billing';\nconsole.log(formatPrice(1));\n",
            ),
            (
                "src/billing.ts",
                "export function formatPrice(value: number) { return `$${value}`; }\n",
            ),
            (
                "src/billing.spec.ts",
                "import { formatPrice } from './billing';\nimport { makePrice } from './factory';\nconsole.log(formatPrice(makePrice()));\n",
            ),
            (
                "src/factory.ts",
                "export function makePrice() { return 42; }\n",
            ),
            ("src/unused-helper.ts", "export const unusedHelper = 1;\n"),
        ],
    );

    assert!(!analysis.issues.files.contains_key("src/billing.spec.ts"));
    assert!(!analysis.issues.files.contains_key("src/factory.ts"));
    assert_no_unused_export(&analysis, "src/factory.ts", "makePrice");
    assert_unused_file(&analysis, "src/unused-helper.ts");
}

#[test]
fn entry_reexports_keep_source_files_reachable_when_entry_exports_are_reported() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r#"{
              "entry": "src/index.ts",
              "project": "src/**/*.ts",
              "includeEntryExports": true
            }"#,
    );
    write_file(
        cwd,
        "src/index.ts",
        "export { used } from './used';\nexport * from './namespace';\n",
    );
    write_file(cwd, "src/used.ts", "export const used = 1;\n");
    write_file(cwd, "src/namespace.ts", "export const value = 1;\n");
    write_file(cwd, "src/dead.ts", "export const dead = 1;\n");

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert!(!analysis.issues.files.contains_key("src/used.ts"));
    assert!(!analysis.issues.files.contains_key("src/namespace.ts"));
    assert!(analysis.issues.files.contains_key("src/dead.ts"));
    assert!(analysis.issues.exports["src/index.ts"].contains_key("used"));
}

#[test]
fn reports_unreachable_files_without_parsing_them() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r#"{
              "entry": "src/main.ts",
              "project": "src/**/*.ts"
            }"#,
    );
    write_file(cwd, "src/main.ts", "console.log('entry');\n");
    write_file(cwd, "src/broken.ts", "export const = ;\n");

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert!(analysis.issues.files.contains_key("src/broken.ts"));
    assert!(!analysis.issues.exports.contains_key("src/broken.ts"));
}

#[test]
fn explicit_aliases_override_package_json_imports() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r##"{
              "entry": "src/main.ts",
              "project": [
                "src/**/*.ts",
                "generated/**/*.ts",
                "wrong/**/*.ts"
              ],
              "aliases": {
                "#generated/*": "./generated/*.ts"
              }
            }"##,
    );
    write_file(
        cwd,
        "package.json",
        r##"{
              "imports": {
                "#generated/*": "./wrong/*.ts"
              }
            }"##,
    );
    write_file(
        cwd,
        "src/main.ts",
        "import { used } from '#generated/used';\nconsole.log(used);\n",
    );
    write_file(cwd, "generated/used.ts", "export const used = 1;\n");
    write_file(cwd, "wrong/used.ts", "export const used = 1;\n");

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert!(analysis.issues.unresolved.is_empty());
    assert!(!analysis.issues.files.contains_key("generated/used.ts"));
    assert!(analysis.issues.files.contains_key("wrong/used.ts"));
    assert!(
        !analysis
            .issues
            .exports
            .get("generated/used.ts")
            .is_some_and(|exports| exports.contains_key("used"))
    );
}

#[test]
fn unresolved_import_modes_control_behavior() {
    let report = analyze_missing_import(None).unwrap();
    assert_eq!(
        report.issues.unresolved["src/main.ts"],
        vec!["./missing".to_string()]
    );
    assert_eq!(report.counters.unresolved, 1);

    let ignore = analyze_missing_import(Some("ignore")).unwrap();
    assert!(ignore.issues.unresolved.is_empty());
    assert_eq!(ignore.counters.unresolved, 0);

    let error = analyze_missing_import(Some("error")).unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("src/main.ts"));
    assert!(message.contains("./missing"));
}

#[test]
fn ignored_unresolved_patterns_do_not_count_as_issues() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r##"{
              "entry": "src/main.ts",
              "project": "src/**/*.ts",
              "unresolvedImports": {
                "ignore": ["#virtual_generated/**"]
              }
            }"##,
    );
    write_file(
        cwd,
        "src/main.ts",
        "import '#virtual_generated/api/foo';\nimport './missing';\n",
    );

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert_eq!(
        analysis.issues.unresolved["src/main.ts"],
        vec!["./missing".to_string()]
    );
    assert_eq!(analysis.counters.unresolved, 1);
}

#[test]
fn verbose_records_ignored_unresolved_patterns_with_samples() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r##"{
              "entry": "src/main.ts",
              "project": "src/**/*.ts",
              "unresolvedImports": {
                "ignore": ["#virtual_generated/**"]
              }
            }"##,
    );
    write_file(cwd, "src/main.ts", "import '#virtual_generated/api/foo';\n");

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(
        cwd,
        &config,
        AnalysisOptions {
            verbose: true,
            ..AnalysisOptions::default()
        },
    )
    .unwrap();

    let ignored = &analysis.ignored_unresolved_imports_by_pattern["#virtual_generated/**"];
    assert_eq!(ignored.count, 1);
    assert_eq!(
        ignored.samples,
        vec![IgnoredUnresolvedImportSample {
            specifier: "#virtual_generated/api/foo".to_string(),
            importer: "src/main.ts".to_string(),
        }]
    );
    assert_eq!(analysis.counters.ignored_unresolved, 1);
}

#[test]
fn resolves_package_imports_before_unresolved_ignore_patterns() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r##"{
              "entry": "src/main.ts",
              "project": "src/**/*.ts",
              "unresolvedImports": {
                "ignore": ["#internal"]
              }
            }"##,
    );
    write_file(
        cwd,
        "package.json",
        r##"{
              "type": "module",
              "imports": {
                "#internal": "./src/internal.ts"
              }
            }"##,
    );
    write_file(
        cwd,
        "src/main.ts",
        "import { used } from '#internal';\nconsole.log(used);\n",
    );
    write_file(
        cwd,
        "src/internal.ts",
        "export const used = 1;\nexport const unused = 2;\n",
    );

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(
        cwd,
        &config,
        AnalysisOptions {
            verbose: true,
            ..AnalysisOptions::default()
        },
    )
    .unwrap();

    assert!(analysis.issues.unresolved.is_empty());
    assert!(analysis.ignored_unresolved_imports_by_pattern.is_empty());
    assert_no_unused_export(&analysis, "src/internal.ts", "used");
    assert_unused_export(&analysis, "src/internal.ts", "unused");
}

#[test]
fn resolves_package_import_js_specifiers_to_ts_source() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r##"{
              "entry": "src/main.ts",
              "project": ["src/**/*.ts", "pplx/**/*.ts"]
            }"##,
    );
    write_file(
        cwd,
        "package.json",
        r##"{
              "type": "module",
              "imports": {
                "#pplx/*": "./pplx/*"
              }
            }"##,
    );
    write_file(
        cwd,
        "src/main.ts",
        "import { used } from '#pplx/frontend/module.js';\nconsole.log(used);\n",
    );
    write_file(
        cwd,
        "pplx/frontend/module.ts",
        "export const used = 1;\nexport const unused = 2;\n",
    );

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert!(analysis.issues.unresolved.is_empty());
    assert_no_unused_export(&analysis, "pplx/frontend/module.ts", "used");
    assert_unused_export(&analysis, "pplx/frontend/module.ts", "unused");
}

#[test]
fn warns_when_unresolved_ignore_overlaps_source_alias() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r##"{
              "entry": "src/main.ts",
              "project": "src/**/*.ts",
              "unresolvedImports": {
                "ignore": ["#pplx/frontend/**"]
              }
            }"##,
    );
    write_file(
        cwd,
        "package.json",
        r##"{
              "imports": {
                "#pplx/*": "./pplx/*.ts"
              }
            }"##,
    );
    write_file(cwd, "src/main.ts", "console.log('entry');\n");

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert_eq!(analysis.source_alias_ignore_warnings.len(), 1);
    assert_eq!(
        analysis.source_alias_ignore_warnings[0].pattern,
        "#pplx/frontend/**"
    );
    assert_eq!(analysis.source_alias_ignore_warnings[0].alias, "#pplx/*");
    assert!(analysis.source_alias_ignore_warnings[0].fix_blocking);
}

#[test]
fn marks_asset_query_source_alias_ignore_as_non_fix_blocking() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r##"{
              "entry": "src/main.ts",
              "project": "src/**/*.ts",
              "unresolvedImports": {
                "ignore": ["#pplx/frontend/**/sprite.generated.svg?raw"]
              }
            }"##,
    );
    write_file(
        cwd,
        "package.json",
        r##"{
              "imports": {
                "#pplx/*": "./pplx/*.ts"
              }
            }"##,
    );
    write_file(cwd, "src/main.ts", "console.log('entry');\n");

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert_eq!(analysis.source_alias_ignore_warnings.len(), 1);
    assert_eq!(
        analysis.source_alias_ignore_warnings[0].pattern,
        "#pplx/frontend/**/sprite.generated.svg?raw"
    );
    assert_eq!(analysis.source_alias_ignore_warnings[0].alias, "#pplx/*");
    assert!(!analysis.source_alias_ignore_warnings[0].fix_blocking);
}

#[test]
fn verbose_explains_unused_exports_with_skipped_importers() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r#"{
              "entry": "src/main.ts",
              "project": "src/**/*.ts",
              "testFilePatterns": "src/**/*.spec.ts"
            }"#,
    );
    write_file(
        cwd,
        "src/main.ts",
        "import { used } from './module';\nconsole.log(used);\n",
    );
    write_file(
        cwd,
        "src/module.ts",
        "export const used = 1;\nexport const onlyForTest = 2;\n",
    );
    write_file(
        cwd,
        "src/module.spec.ts",
        "import { onlyForTest } from './module';\nconsole.log(onlyForTest);\n",
    );

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(
        cwd,
        &config,
        AnalysisOptions {
            verbose: true,
            ..AnalysisOptions::default()
        },
    )
    .unwrap();

    let explanation = analysis.issues.exports["src/module.ts"]["onlyForTest"]
        .explanation
        .as_ref()
        .unwrap();
    assert!(explanation.file_reachable);
    assert!(explanation.importers_considered.is_empty());
    assert_eq!(
        explanation.importers_skipped,
        vec![SkippedImporterExplanation {
            importer: "src/module.spec.ts".to_string(),
            specifier: "./module".to_string(),
            reason: "test file leaf".to_string(),
        }]
    );
}

#[test]
fn only_import_meta_glob_marks_globbed_files_reachable() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    write_file(
        cwd,
        "codescythe.json",
        r#"{
              "entry": "src/main.ts",
              "project": "src/**/*.ts"
            }"#,
    );
    write_file(
        cwd,
        "src/main.ts",
        r#"const holder = { meta: { glob: (_pattern: string) => ({}) } };
holder.meta.glob("./routes/*.ts");
"#,
    );
    write_file(cwd, "src/routes/home.ts", "export const route = 1;\n");

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert!(analysis.issues.files.contains_key("src/routes/home.ts"));
}

#[test]
fn reports_missing_local_imports() {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();

    fs::create_dir_all(cwd.join("app")).unwrap();
    fs::write(
        cwd.join("codescythe.json"),
        r#"{
              "entry": "app/index.ts",
              "project": "app/**/*.ts"
            }"#,
    )
    .unwrap();
    fs::write(
        cwd.join("app/index.ts"),
        r#"import './missing';
import missingExternal from 'missing-external';
import missingExternalSubpath from 'missing-external/subpath';

console.log(missingExternal, missingExternalSubpath);
"#,
    )
    .unwrap();

    let config = crate::load_config(cwd, None).unwrap();
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default()).unwrap();

    assert_eq!(
        analysis.issues.unresolved["app/index.ts"],
        vec!["./missing".to_string()]
    );
}

fn analyze_missing_import(mode: Option<&str>) -> Result<Analysis> {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();
    let mode_config = mode
        .map(|mode| format!(r#", "unresolvedImports": {{ "mode": "{mode}" }}"#))
        .unwrap_or_default();

    write_file(
        cwd,
        "codescythe.json",
        &format!(
            r#"{{
                  "entry": "src/main.ts",
                  "project": "src/**/*.ts"{mode_config}
                }}"#
        ),
    );
    write_file(cwd, "src/main.ts", "import './missing';\n");

    let config = crate::load_config(cwd, None).unwrap();
    analyze_path(cwd, &config, AnalysisOptions::default())
}

fn analyze_inline_project(files: &[(&str, &str)]) -> Analysis {
    analyze_inline_project_with_config(
        r#"{
              "entry": "src/entry.ts",
              "project": "src/**/*.ts"
            }"#,
        files,
    )
}

fn analyze_inline_project_with_config(config: &str, files: &[(&str, &str)]) -> Analysis {
    let tempdir = tempfile::tempdir().unwrap();
    let cwd = tempdir.path();
    write_file(cwd, "codescythe.json", config);
    for (relative, contents) in files {
        write_file(cwd, relative, contents);
    }

    let config = crate::load_config(cwd, None).unwrap();
    analyze_path(cwd, &config, AnalysisOptions::default()).unwrap()
}

fn assert_no_unused_export(analysis: &Analysis, path: &str, name: &str) {
    assert!(
        !analysis
            .issues
            .exports
            .get(path)
            .is_some_and(|exports| exports.contains_key(name)),
        "expected {path}:{name} to be used, got {:?}",
        analysis.issues.exports.get(path)
    );
}

fn assert_unused_export(analysis: &Analysis, path: &str, name: &str) {
    assert!(
        analysis
            .issues
            .exports
            .get(path)
            .is_some_and(|exports| exports.contains_key(name)),
        "expected {path}:{name} to be unused, got {:?}",
        analysis.issues.exports.get(path)
    );
}

fn assert_unused_file(analysis: &Analysis, path: &str) {
    assert!(
        analysis.issues.files.contains_key(path),
        "expected {path} to be unused, got {:?}",
        analysis.issues.files
    );
}

fn write_file(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn fixture_path(name: &str) -> (tempfile::TempDir, PathBuf) {
    let relative = Path::new("tests/fixtures").join(name);
    let mut candidates = vec![
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(&relative),
    ];

    if let Ok(test_srcdir) = env::var("TEST_SRCDIR") {
        let test_srcdir = PathBuf::from(test_srcdir);
        let workspace = env::var("TEST_WORKSPACE").unwrap_or_else(|_| "_main".to_string());
        candidates.push(test_srcdir.join(workspace).join(&relative));
        candidates.push(test_srcdir.join("_main").join(&relative));
    }

    if let Ok(current_dir) = env::current_dir() {
        candidates.push(current_dir.join(&relative));
    }

    for candidate in &candidates {
        if candidate.exists() {
            let tempdir = tempfile::tempdir().unwrap();
            let target = tempdir.path().join(name);
            copy_fixture(candidate, &target);
            return (tempdir, target);
        }
    }

    panic!(
        "failed to locate fixture {name}; tried: {}",
        candidates
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn copy_fixture(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in WalkDir::new(source).follow_links(true) {
        let entry = entry.unwrap();
        let relative = entry.path().strip_prefix(source).unwrap();
        let output = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&output).unwrap();
        } else if entry.file_type().is_file() {
            fs::copy(entry.path(), output).unwrap();
        }
    }
}
