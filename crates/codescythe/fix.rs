use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{Analysis, SymbolIssue};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixResult {
    pub changed_files: Vec<String>,
    pub removed_exports: usize,
    pub analysis: Analysis,
}

pub fn apply_fixes(cwd: &Path, analysis: &Analysis) -> Result<FixResult> {
    let mut changed_files = Vec::new();
    let mut removed_exports = 0;

    for (relative, exports) in &analysis.issues.exports {
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
