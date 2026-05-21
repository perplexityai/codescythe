use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    Analysis, CodescytheConfig, SymbolIssue, analyze::ignored_unresolved_patterns_for_file,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixResult {
    pub changed_files: Vec<String>,
    pub removed_files: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped_export_files: Vec<String>,
    pub removed_exports: usize,
    pub analysis: Analysis,
}

pub fn apply_fixes(cwd: &Path, analysis: &Analysis) -> Result<FixResult> {
    apply_fixes_internal(cwd, None, analysis, true)
}

pub fn apply_fixes_with_options(
    cwd: &Path,
    config: &CodescytheConfig,
    analysis: &Analysis,
    force: bool,
) -> Result<FixResult> {
    apply_fixes_internal(cwd, Some(config), analysis, force)
}

fn apply_fixes_internal(
    cwd: &Path,
    config: Option<&CodescytheConfig>,
    analysis: &Analysis,
    force: bool,
) -> Result<FixResult> {
    let mut changed_files = Vec::new();
    let mut removed_files = Vec::new();
    let mut skipped_export_files = Vec::new();
    let mut removed_exports = 0;
    let files_to_remove = analysis
        .issues
        .files
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();

    for relative in &files_to_remove {
        let path = cwd.join(relative);
        fs::remove_file(&path)
            .with_context(|| format!("failed to remove unused file {}", path.display()))?;
        removed_files.push(relative.clone());
    }

    for (relative, exports) in &analysis.issues.exports {
        if files_to_remove.contains(relative) {
            continue;
        }

        if !force
            && let Some(config) = config
            && !analysis.ignored_unresolved_imports_by_pattern.is_empty()
            && !ignored_unresolved_patterns_for_file(
                relative,
                cwd,
                config,
                &analysis.ignored_unresolved_imports_by_pattern,
            )?
            .is_empty()
        {
            skipped_export_files.push(relative.clone());
            continue;
        }

        let path = cwd.join(relative);
        let source = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {} while applying fixes", path.display()))?;
        let ranges = removal_ranges(&source, exports.values());
        if ranges.is_empty() {
            continue;
        }

        let mut output = source;
        for (start, end) in ranges.into_iter().rev() {
            output.replace_range(start..end, "");
            removed_exports += 1;
        }

        fs::write(&path, output)
            .with_context(|| format!("failed to write fixed file {}", path.display()))?;
        changed_files.push(relative.clone());
    }

    Ok(FixResult {
        changed_files,
        removed_files,
        skipped_export_files,
        removed_exports,
        analysis: analysis.clone(),
    })
}

fn removal_ranges<'a>(
    source: &str,
    issues: impl Iterator<Item = &'a SymbolIssue>,
) -> Vec<(usize, usize)> {
    let mut ranges = BTreeSet::new();
    for issue in issues {
        let start = issue.span.0 as usize;
        let end = issue.span.1 as usize;
        if start >= end || end > source.len() {
            continue;
        }
        ranges.insert(expand_to_full_lines(source, start, end));
    }

    let mut merged = Vec::<(usize, usize)>::new();
    for (start, end) in ranges {
        match merged.last_mut() {
            Some((_, last_end)) if start <= *last_end => {
                *last_end = (*last_end).max(end);
            }
            _ => merged.push((start, end)),
        }
    }
    merged
}

fn expand_to_full_lines(source: &str, start: usize, end: usize) -> (usize, usize) {
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[end..]
        .find('\n')
        .map_or(source.len(), |index| end + index + 1);
    (line_start, line_end)
}

#[allow(dead_code)]
fn _assert_sorted_map_send_sync(_: &BTreeMap<String, BTreeMap<String, SymbolIssue>>) {}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs};

    use crate::analyze::ExportKind;
    use crate::{
        Analysis, CodescytheConfig, FileIssue, IgnoredUnresolvedImportSample,
        IgnoredUnresolvedImportsByPattern, Issues, SymbolIssue, UnresolvedImportsConfig,
        apply_fixes, apply_fixes_with_options, run_and_fix,
    };

    #[test]
    fn removes_unused_files_and_exports() {
        let tempdir = tempfile::tempdir().unwrap();
        let cwd = tempdir.path();
        fs::write(
            cwd.join("codescythe.json"),
            r#"{
              "entry": ["src/main.ts"],
              "project": ["src/**/*.ts"]
            }"#,
        )
        .unwrap();
        fs::create_dir(cwd.join("src")).unwrap();
        fs::write(
            cwd.join("src/main.ts"),
            "import { used } from './used';\nconsole.log(used);\n",
        )
        .unwrap();
        fs::write(
            cwd.join("src/used.ts"),
            "export const used = 1;\nexport const unused = 2;\n",
        )
        .unwrap();
        fs::write(cwd.join("src/dead.ts"), "export const dead = 1;\n").unwrap();

        let result = run_and_fix(cwd, None).unwrap();

        assert_eq!(result.changed_files, vec!["src/used.ts"]);
        assert_eq!(result.removed_files, vec!["src/dead.ts"]);
        assert_eq!(result.removed_exports, 1);
        assert!(!cwd.join("src/dead.ts").exists());
        assert_eq!(
            fs::read_to_string(cwd.join("src/used.ts")).unwrap(),
            "export const used = 1;\n"
        );
    }

    #[test]
    fn skips_export_edits_for_removed_files() {
        let tempdir = tempfile::tempdir().unwrap();
        let cwd = tempdir.path();
        fs::write(cwd.join("dead.ts"), "export const dead = 1;\n").unwrap();

        let mut files = BTreeMap::new();
        files.insert(
            "dead.ts".to_string(),
            FileIssue {
                path: "dead.ts".to_string(),
            },
        );
        let mut dead_exports = BTreeMap::new();
        dead_exports.insert(
            "dead".to_string(),
            SymbolIssue {
                symbol: "dead".to_string(),
                kind: ExportKind::Value,
                line: 1,
                col: 14,
                explanation: None,
                span: (0, 23),
            },
        );
        let mut exports = BTreeMap::new();
        exports.insert("dead.ts".to_string(), dead_exports);

        let result = apply_fixes(
            cwd,
            &Analysis {
                issues: Issues {
                    files,
                    exports,
                    unresolved: BTreeMap::new(),
                },
                counters: Default::default(),
                summary: None,
                ignored_unresolved_imports_by_pattern: BTreeMap::new(),
                source_alias_ignore_warnings: Vec::new(),
                explain_export: None,
            },
        )
        .unwrap();

        assert_eq!(result.removed_files, vec!["dead.ts"]);
        assert!(result.changed_files.is_empty());
        assert_eq!(result.removed_exports, 0);
        assert!(!cwd.join("dead.ts").exists());
    }

    #[test]
    fn removes_tests_that_reference_removed_exports() {
        let tempdir = tempfile::tempdir().unwrap();
        let cwd = tempdir.path();
        fs::write(
            cwd.join("codescythe.json"),
            r#"{
              "entry": ["src/main.ts", "src/**/*.spec.ts"],
              "project": ["src/**/*.ts"],
              "testFilePatterns": "src/**/*.spec.ts"
            }"#,
        )
        .unwrap();
        fs::create_dir(cwd.join("src")).unwrap();
        fs::write(
            cwd.join("src/main.ts"),
            "import { used } from './module';\nconsole.log(used);\n",
        )
        .unwrap();
        fs::write(
            cwd.join("src/module.ts"),
            "export const used = 1;\nexport const onlyForTest = 2;\n",
        )
        .unwrap();
        fs::write(
            cwd.join("src/module.spec.ts"),
            "import { onlyForTest } from './module';\nconsole.log(onlyForTest);\n",
        )
        .unwrap();

        let result = run_and_fix(cwd, None).unwrap();

        assert_eq!(result.changed_files, vec!["src/module.ts"]);
        assert_eq!(result.removed_files, vec!["src/module.spec.ts"]);
        assert_eq!(result.removed_exports, 1);
        assert!(!cwd.join("src/module.spec.ts").exists());
        assert_eq!(
            fs::read_to_string(cwd.join("src/module.ts")).unwrap(),
            "export const used = 1;\n"
        );
    }

    #[test]
    fn refuses_fix_when_unresolved_ignore_overlaps_source_alias_without_force() {
        let tempdir = tempfile::tempdir().unwrap();
        let cwd = tempdir.path();
        fs::write(
            cwd.join("codescythe.json"),
            r##"{
              "entry": "src/main.ts",
              "project": ["src/**/*.ts", "workspace/**/*.ts"],
              "unresolvedImports": {
                "ignore": ["#workspace/frontend/**"]
              }
            }"##,
        )
        .unwrap();
        fs::write(
            cwd.join("package.json"),
            r##"{
              "imports": {
                "#workspace/*": "./workspace/*.ts"
              }
            }"##,
        )
        .unwrap();
        fs::create_dir_all(cwd.join("src")).unwrap();
        fs::write(cwd.join("src/main.ts"), "console.log('entry');\n").unwrap();

        let error = run_and_fix(cwd, None).unwrap_err();
        let message = format!("{error:#}");

        assert!(message.contains("unresolvedImports.ignore overlaps local source aliases"));
    }

    #[test]
    fn permits_fix_when_source_alias_ignore_only_matches_asset_query_imports() {
        let tempdir = tempfile::tempdir().unwrap();
        let cwd = tempdir.path();
        fs::write(
            cwd.join("codescythe.json"),
            r##"{
              "entry": "src/main.ts",
              "project": ["src/**/*.ts", "workspace/**/*.ts"],
              "unresolvedImports": {
                "ignore": ["#workspace/frontend/**/sprite.generated.svg?raw"]
              }
            }"##,
        )
        .unwrap();
        fs::write(
            cwd.join("package.json"),
            r##"{
              "imports": {
                "#workspace/*": "./workspace/*.ts"
              }
            }"##,
        )
        .unwrap();
        fs::create_dir_all(cwd.join("src")).unwrap();
        fs::write(
            cwd.join("src/main.ts"),
            "import sprite from '#workspace/frontend/app/sprite.generated.svg?raw';\nconsole.log(sprite);\n",
        )
        .unwrap();
        fs::write(cwd.join("src/dead.ts"), "export const dead = 1;\n").unwrap();

        let result = run_and_fix(cwd, None).unwrap();

        assert_eq!(result.removed_files, vec!["src/dead.ts"]);
        assert_eq!(result.analysis.counters.ignored_unresolved, 1);
        assert_eq!(result.analysis.source_alias_ignore_warnings.len(), 1);
        assert!(!result.analysis.source_alias_ignore_warnings[0].fix_blocking);
    }

    #[test]
    fn skips_export_edits_when_ignored_unresolved_import_might_point_to_file() {
        let tempdir = tempfile::tempdir().unwrap();
        let cwd = tempdir.path();
        fs::create_dir_all(cwd.join("src")).unwrap();
        fs::write(cwd.join("src/module.ts"), "export const maybeUsed = 1;\n").unwrap();

        let mut aliases = BTreeMap::new();
        aliases.insert("#app/*".to_string(), vec!["./src/*.ts".to_string()]);
        let config = CodescytheConfig {
            aliases,
            unresolved_imports: UnresolvedImportsConfig {
                ignore: vec!["#app/module".to_string()],
                ..UnresolvedImportsConfig::default()
            },
            ..CodescytheConfig::default()
        };

        let mut module_exports = BTreeMap::new();
        module_exports.insert(
            "maybeUsed".to_string(),
            SymbolIssue {
                symbol: "maybeUsed".to_string(),
                kind: ExportKind::Value,
                line: 1,
                col: 14,
                explanation: None,
                span: (0, 27),
            },
        );
        let mut exports = BTreeMap::new();
        exports.insert("src/module.ts".to_string(), module_exports);
        let mut ignored = BTreeMap::new();
        ignored.insert(
            "#app/module".to_string(),
            IgnoredUnresolvedImportsByPattern {
                pattern: "#app/module".to_string(),
                count: 1,
                samples: vec![IgnoredUnresolvedImportSample {
                    specifier: "#app/module".to_string(),
                    importer: "src/entry.ts".to_string(),
                }],
            },
        );

        let result = apply_fixes_with_options(
            cwd,
            &config,
            &Analysis {
                issues: Issues {
                    files: BTreeMap::new(),
                    exports,
                    unresolved: BTreeMap::new(),
                },
                counters: Default::default(),
                summary: None,
                ignored_unresolved_imports_by_pattern: ignored,
                source_alias_ignore_warnings: Vec::new(),
                explain_export: None,
            },
            false,
        )
        .unwrap();

        assert_eq!(result.skipped_export_files, vec!["src/module.ts"]);
        assert!(result.changed_files.is_empty());
        assert_eq!(
            fs::read_to_string(cwd.join("src/module.ts")).unwrap(),
            "export const maybeUsed = 1;\n"
        );
    }
}
