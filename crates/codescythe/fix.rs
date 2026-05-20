use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{Analysis, ImportRewrite, SymbolIssue};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixResult {
    pub changed_files: Vec<String>,
    pub removed_files: Vec<String>,
    pub removed_exports: usize,
    pub analysis: Analysis,
}

pub fn apply_fixes(cwd: &Path, analysis: &Analysis) -> Result<FixResult> {
    let mut changed_files = BTreeSet::new();
    let mut removed_files = Vec::new();
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

    apply_import_rewrites(
        cwd,
        &analysis.import_rewrites,
        &files_to_remove,
        &mut changed_files,
    )?;

    for (relative, exports) in &analysis.issues.exports {
        if files_to_remove.contains(relative) {
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
        changed_files.insert(relative.clone());
    }

    Ok(FixResult {
        changed_files: changed_files.into_iter().collect(),
        removed_files,
        removed_exports,
        analysis: analysis.clone(),
    })
}

fn apply_import_rewrites(
    cwd: &Path,
    rewrites: &[ImportRewrite],
    removed_files: &BTreeSet<String>,
    changed_files: &mut BTreeSet<String>,
) -> Result<()> {
    let mut rewrites_by_file = BTreeMap::<String, Vec<&ImportRewrite>>::new();
    for rewrite in rewrites {
        if !removed_files.contains(&rewrite.file) {
            rewrites_by_file
                .entry(rewrite.file.clone())
                .or_default()
                .push(rewrite);
        }
    }

    for (relative, mut rewrites) in rewrites_by_file {
        let path = cwd.join(&relative);
        let source = fs::read_to_string(&path).with_context(|| {
            format!(
                "failed to read {} while applying import rewrites",
                path.display()
            )
        })?;
        let mut output = source.clone();
        rewrites.sort_by_key(|rewrite| rewrite.source_span.0);
        for rewrite in rewrites.into_iter().rev() {
            let start = rewrite.source_span.0 as usize;
            let end = rewrite.source_span.1 as usize;
            if start >= end || end > output.len() {
                continue;
            }
            let quote = output[start..end]
                .chars()
                .next()
                .filter(|ch| *ch == '"' || *ch == '\'')
                .unwrap_or('\'');
            output.replace_range(
                start..end,
                &quoted_module_source(&rewrite.replacement_source, quote),
            );
        }

        if output != source {
            fs::write(&path, output)
                .with_context(|| format!("failed to write fixed file {}", path.display()))?;
            changed_files.insert(relative);
        }
    }

    Ok(())
}

fn quoted_module_source(source: &str, quote: char) -> String {
    let escaped = source.replace(quote, &format!("\\{quote}"));
    format!("{quote}{escaped}{quote}")
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
    use crate::{Analysis, FileIssue, Issues, SymbolIssue, apply_fixes, run_and_fix};

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
                span: (0, 23),
                reexport_source: None,
                reexport_name: None,
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
                import_rewrites: Vec::new(),
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
    fn rewrites_test_type_imports_for_removed_type_reexports() {
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
            "import { useT } from './a.js';\nconsole.log(useT({ id: 'x' }));\n",
        )
        .unwrap();
        fs::write(
            cwd.join("src/types.ts"),
            "export type T = { id: string };\n",
        )
        .unwrap();
        fs::write(
            cwd.join("src/a.ts"),
            "import type { T } from './types.js';\n\nexport type { T } from './types.js';\n\nexport function useT(value: T): string {\n  return value.id;\n}\n",
        )
        .unwrap();
        fs::write(
            cwd.join("src/a.test.ts"),
            "import { useT } from './a.js';\nimport type { T } from './a.js';\n\nconst value: T = { id: 'x' };\nuseT(value);\n",
        )
        .unwrap();

        let result = run_and_fix(cwd, None).unwrap();

        assert_eq!(result.removed_exports, 1);
        assert!(result.removed_files.is_empty());
        assert!(result.changed_files.contains(&"src/a.ts".to_string()));
        assert!(result.changed_files.contains(&"src/a.test.ts".to_string()));
        assert_eq!(
            fs::read_to_string(cwd.join("src/a.test.ts")).unwrap(),
            "import { useT } from './a.js';\nimport type { T } from './types.js';\n\nconst value: T = { id: 'x' };\nuseT(value);\n"
        );
        let a_source = fs::read_to_string(cwd.join("src/a.ts")).unwrap();
        assert!(!a_source.contains("export type { T }"));
        assert!(a_source.contains("export function useT(value: T): string"));
        assert!(cwd.join("src/a.test.ts").exists());

        let post_fix = crate::run(cwd, None).unwrap();
        assert!(post_fix.issues.files.is_empty());
        assert!(post_fix.issues.exports.is_empty());
    }
}
