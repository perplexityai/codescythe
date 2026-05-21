use super::*;

#[derive(Default)]
pub(super) struct ExportUsageIndex {
    considered: HashMap<ExportUsageKey, Vec<ExportImportExplanation>>,
    skipped: HashMap<ExportUsageKey, Vec<SkippedImporterExplanation>>,
}

pub(super) fn build_export_usage_index(
    files: &mut FileCache,
    resolver: &ModuleResolver,
    used_files: &UsedFiles,
    entry_indexes: &HashSet<usize>,
    test_file_indexes: &TestFiles,
) -> Result<ExportUsageIndex> {
    let mut usage = ExportUsageIndex::default();
    for index in 0..files.paths.len() {
        let Ok(file) = files.try_get(index) else {
            continue;
        };
        let file = file.clone();
        let skip_reason = if test_file_indexes.contains(&index) {
            Some("test file leaf".to_string())
        } else if !used_files.contains(&index) && !entry_indexes.contains(&index) {
            Some("importer unreachable".to_string())
        } else {
            None
        };

        for import in &file.imports {
            let Some(imported) = &import.imported else {
                continue;
            };
            if let ImportResolution::Project(target) = resolver.resolve(&file, &import.source)? {
                add_export_usage(
                    &mut usage,
                    target,
                    imported,
                    &file.relative,
                    &import.source,
                    "named import",
                    skip_reason.as_deref(),
                );
            }
        }

        for (local, member) in &file.member_uses {
            let Some(source) = file.namespace_imports.get(local) else {
                continue;
            };
            if let ImportResolution::Project(target) = resolver.resolve(&file, source)? {
                add_export_usage(
                    &mut usage,
                    target,
                    member,
                    &file.relative,
                    source,
                    "namespace member access",
                    skip_reason.as_deref(),
                );
            }
        }

        for export in file.exports.values() {
            if let (Some(source), Some(name)) = (&export.reexport_source, &export.reexport_name)
                && let ImportResolution::Project(target) = resolver.resolve(&file, source)?
            {
                add_export_usage(
                    &mut usage,
                    target,
                    name,
                    &file.relative,
                    source,
                    "re-export",
                    skip_reason.as_deref(),
                );
            }
        }

        for source in &file.dynamic_imports {
            if let ImportResolution::Project(target) = resolver.resolve(&file, source)? {
                add_all_export_usage(
                    files,
                    &mut usage,
                    target,
                    &file.relative,
                    source,
                    "dynamic import marks all exports",
                    skip_reason.as_deref(),
                )?;
            }
        }

        for source in &file.reexport_all {
            if let ImportResolution::Project(target) = resolver.resolve(&file, source)? {
                add_all_export_usage(
                    files,
                    &mut usage,
                    target,
                    &file.relative,
                    source,
                    "export star marks all exports",
                    skip_reason.as_deref(),
                )?;
            }
        }
    }
    Ok(usage)
}

fn add_all_export_usage(
    files: &mut FileCache,
    usage: &mut ExportUsageIndex,
    target: usize,
    importer: &str,
    specifier: &str,
    reason: &str,
    skip_reason: Option<&str>,
) -> Result<()> {
    let export_names = files
        .try_get(target)
        .map(|file| file.exports.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    for export_name in export_names {
        add_export_usage(
            usage,
            target,
            &export_name,
            importer,
            specifier,
            reason,
            skip_reason,
        );
    }
    Ok(())
}

fn add_export_usage(
    usage: &mut ExportUsageIndex,
    target: usize,
    symbol: &str,
    importer: &str,
    specifier: &str,
    reason: &str,
    skip_reason: Option<&str>,
) {
    let key = (target, symbol.to_string());
    if let Some(skip_reason) = skip_reason {
        usage
            .skipped
            .entry(key)
            .or_default()
            .push(SkippedImporterExplanation {
                importer: importer.to_string(),
                specifier: specifier.to_string(),
                reason: skip_reason.to_string(),
            });
    } else {
        usage
            .considered
            .entry(key)
            .or_default()
            .push(ExportImportExplanation {
                importer: importer.to_string(),
                specifier: specifier.to_string(),
                reason: reason.to_string(),
            });
    }
}

pub(super) fn add_export_explanations(
    issues: &mut Issues,
    files: &FileCache,
    used_files: &UsedFiles,
    usage: &ExportUsageIndex,
    aliases: &[AliasMapping],
    ignored: &BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
) {
    for (relative, exports) in &mut issues.exports {
        let Some(index) = files.index_by_relative(relative) else {
            continue;
        };
        for issue in exports.values_mut() {
            issue.explanation = Some(export_explanation(
                files,
                index,
                &issue.symbol,
                used_files,
                usage,
                aliases,
                ignored,
            ));
        }
    }
}

pub(super) fn explain_requested_export(
    request: &ExplainExportRequest,
    files: &mut FileCache,
    issues: &Issues,
    used_files: &UsedFiles,
    usage: Option<&ExportUsageIndex>,
    aliases: &[AliasMapping],
    ignored: &BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
) -> Result<ExplainExportResult> {
    let Some(index) = files.index_by_relative(&request.file) else {
        return Ok(ExplainExportResult {
            exporting_file: request.file.clone(),
            symbol: request.symbol.clone(),
            status: ExplainExportStatus::FileNotFound,
            reason: "file is outside the analyzed project set".to_string(),
            explanation: None,
        });
    };

    let file = match files.try_get(index) {
        Ok(file) => file,
        Err(error) => {
            return Ok(ExplainExportResult {
                exporting_file: request.file.clone(),
                symbol: request.symbol.clone(),
                status: ExplainExportStatus::FileUnused,
                reason: format!("file is unreachable and could not be parsed: {error}"),
                explanation: None,
            });
        }
    };
    if !file.exports.contains_key(&request.symbol) {
        return Ok(ExplainExportResult {
            exporting_file: request.file.clone(),
            symbol: request.symbol.clone(),
            status: ExplainExportStatus::SymbolNotExported,
            reason: "symbol is not exported by the requested file".to_string(),
            explanation: None,
        });
    }

    let usage = usage.expect("export usage is built when explain_export is requested");
    let explanation = export_explanation(
        files,
        index,
        &request.symbol,
        used_files,
        usage,
        aliases,
        ignored,
    );
    let status = if issues
        .exports
        .get(&request.file)
        .is_some_and(|exports| exports.contains_key(&request.symbol))
    {
        ExplainExportStatus::Dead
    } else if issues.files.contains_key(&request.file) {
        ExplainExportStatus::FileUnused
    } else {
        ExplainExportStatus::Alive
    };
    let reason = match status {
        ExplainExportStatus::Alive => {
            if explanation.importers_considered.is_empty() {
                "export is kept alive by entry/public-file semantics".to_string()
            } else {
                "export is used by reachable importers".to_string()
            }
        }
        ExplainExportStatus::Dead => "export is not used by reachable importers".to_string(),
        ExplainExportStatus::FileUnused => "exporting file is unreachable".to_string(),
        ExplainExportStatus::FileNotFound | ExplainExportStatus::SymbolNotExported => {
            unreachable!("handled before explanation")
        }
    };

    Ok(ExplainExportResult {
        exporting_file: request.file.clone(),
        symbol: request.symbol.clone(),
        status,
        reason,
        explanation: Some(explanation),
    })
}

fn export_explanation(
    files: &FileCache,
    index: usize,
    symbol: &str,
    used_files: &UsedFiles,
    usage: &ExportUsageIndex,
    aliases: &[AliasMapping],
    ignored: &BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
) -> ExportExplanation {
    let mut importers_considered = usage
        .considered
        .get(&(index, symbol.to_string()))
        .cloned()
        .unwrap_or_default();
    importers_considered.sort();
    importers_considered.dedup();

    let mut importers_skipped = usage
        .skipped
        .get(&(index, symbol.to_string()))
        .cloned()
        .unwrap_or_default();
    importers_skipped.sort();
    importers_skipped.dedup();

    ExportExplanation {
        exporting_file: files.relative(index),
        symbol: symbol.to_string(),
        file_reachable: used_files.contains(&index),
        importers_considered,
        importers_skipped,
        ignored_unresolved_imports_that_might_have_pointed_at_this_file:
            ignored_unresolved_for_file(&files.relative(index), aliases, ignored),
    }
}

fn ignored_unresolved_for_file(
    relative: &str,
    aliases: &[AliasMapping],
    ignored: &BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
) -> Vec<IgnoredUnresolvedImportSample> {
    let candidates = alias_specifier_candidates(relative, aliases);
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut samples = BTreeSet::new();
    for ignored_by_pattern in ignored.values() {
        let Ok(pattern) = Glob::new(&ignored_by_pattern.pattern).map(|glob| glob.compile_matcher())
        else {
            continue;
        };
        let pattern_matches_candidate = candidates
            .iter()
            .any(|candidate| pattern.is_match(candidate.as_str()));
        for sample in &ignored_by_pattern.samples {
            if pattern_matches_candidate
                || candidates
                    .iter()
                    .any(|candidate| sample.specifier.starts_with(candidate))
            {
                samples.insert(sample.clone());
            }
        }
    }
    samples.into_iter().collect()
}

pub fn ignored_unresolved_patterns_for_file(
    relative: &str,
    cwd: &Path,
    config: &CodescytheConfig,
    ignored: &BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
) -> Result<Vec<String>> {
    let aliases = source_alias_mappings(cwd, config)?;
    let candidates = alias_specifier_candidates(relative, &aliases);
    let mut patterns = Vec::new();
    for pattern in ignored.keys() {
        let matcher = Glob::new(pattern)
            .with_context(|| format!("invalid glob pattern {pattern:?}"))?
            .compile_matcher();
        if candidates
            .iter()
            .any(|candidate| matcher.is_match(candidate))
        {
            patterns.push(pattern.clone());
        }
    }
    Ok(patterns)
}

fn alias_specifier_candidates(relative: &str, aliases: &[AliasMapping]) -> Vec<String> {
    let mut candidates = BTreeSet::new();
    for alias in aliases {
        for value in &alias.values {
            if let Some(candidate) = alias_specifier_candidate(relative, &alias.key, value) {
                candidates.insert(candidate.clone());
                candidates.insert(strip_known_extension(&candidate).to_string());
            }
        }
    }
    candidates.into_iter().collect()
}

fn alias_specifier_candidate(relative: &str, key: &str, value: &str) -> Option<String> {
    let wildcard = value.find('*')?;
    let before = normalize_alias_target(&value[..wildcard]);
    let after = normalize_alias_target(&value[wildcard + 1..]);
    let relative = relative.replace('\\', "/");
    let tail = relative.strip_prefix(&before)?.strip_suffix(&after)?;

    if let Some(key_wildcard) = key.find('*') {
        Some(format!(
            "{}{}{}",
            &key[..key_wildcard],
            tail,
            &key[key_wildcard + 1..]
        ))
    } else {
        Some(key.to_string())
    }
}

fn normalize_alias_target(value: &str) -> String {
    value
        .trim_start_matches("./")
        .trim_start_matches('/')
        .replace('\\', "/")
}

fn strip_known_extension(value: &str) -> &str {
    for extension in [".tsx", ".ts", ".mts", ".cts", ".jsx", ".js", ".mjs", ".cjs"] {
        if let Some(stripped) = value.strip_suffix(extension) {
            return stripped;
        }
    }
    value
}
