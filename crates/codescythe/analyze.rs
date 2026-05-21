mod discovery;
mod doctor;
mod explain;
mod graph;
mod parse;
mod resolver;
mod util;

#[cfg(test)]
mod tests;

pub use doctor::doctor_config;
pub use explain::ignored_unresolved_patterns_for_file;
pub use resolver::{
    source_alias_fix_blocking_ignore_warnings_for_config, source_alias_ignore_warnings_for_config,
};

use discovery::{discover_entry_files, discover_project_files, discover_test_file_indexes};
use explain::{add_export_explanations, build_export_usage_index, explain_requested_export};
use graph::{
    discover_live_test_support_files, discover_removable_test_files, mark_all_exports,
    mark_glob_import, mark_member_import, mark_reexport, mark_reexport_source_file,
    mark_source_file, mark_used_export, mark_used_file,
};
use parse::{ExportInfo, FileCache, FileData};
use resolver::{
    ImportResolution, ModuleResolver, UnresolvedImportPolicy, package_import_keys,
    source_alias_ignore_warnings, source_alias_mappings,
};
use util::{
    absolute_normalize_path, build_glob_set, has_glob_meta, normalize_path,
    project_glob_from_import, relative_path, should_enter,
};

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    env, fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
    thread,
};

use anyhow::{Context, Result};
use globset::{Glob, GlobMatcher, GlobSet, GlobSetBuilder};
use ignore::{DirEntry, WalkBuilder};
use oxc::ast_visit::{Visit, walk};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::{Parser, ParserReturn};
use oxc_resolver::{AliasValue, ResolveError, ResolveOptions, Resolver, TsconfigDiscovery};
use oxc_span::{GetSpan, SourceType, Span};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{CodescytheConfig, UnresolvedImportsMode};

const PARSE_THREADS_ENV: &str = "CODESCYTHE_PARSE_THREADS";
const RAYON_THREADS_ENV: &str = "RAYON_NUM_THREADS";
const IGNORED_UNRESOLVED_SAMPLE_LIMIT: usize = 5;

type UsedFiles = HashSet<usize>;
type UsedExports = HashMap<usize, HashSet<String>>;
type QueuedFiles = HashSet<usize>;
type TestFiles = HashSet<usize>;
type UnresolvedImports = HashMap<String, HashSet<String>>;
type ExportUsageKey = (usize, String);

#[derive(Debug, Clone, Default)]
pub struct AnalysisOptions {
    pub include_unreachable_exports: bool,
    pub verbose: bool,
    pub retain_ignored_unresolved: bool,
    pub config_path: Option<PathBuf>,
    pub explain_export: Option<ExplainExportRequest>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub issues: Issues,
    pub counters: Counters,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<AnalysisSummary>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub ignored_unresolved_imports_by_pattern: BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub source_alias_ignore_warnings: Vec<SourceAliasIgnoreWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explain_export: Option<ExplainExportResult>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issues {
    pub files: BTreeMap<String, FileIssue>,
    pub exports: BTreeMap<String, BTreeMap<String, SymbolIssue>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub unresolved: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIssue {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolIssue {
    pub symbol: String,
    pub kind: ExportKind,
    pub line: usize,
    pub col: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<ExportExplanation>,
    #[serde(skip)]
    pub span: (u32, u32),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum ExportKind {
    Value,
    Type,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counters {
    pub files: usize,
    pub exports: usize,
    pub unresolved: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub ignored_unresolved: usize,
    pub processed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSummary {
    pub version: String,
    pub config_path: Option<String>,
    pub project_count: usize,
    pub entry_count: usize,
    pub ignored_unresolved_count: usize,
    pub ignored_unresolved_patterns: Vec<String>,
    pub package_import_keys: Vec<String>,
    pub configured_alias_keys: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDoctorResult {
    pub warnings: Vec<ConfigDoctorWarning>,
    pub summary: AnalysisSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDoctorWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoredUnresolvedImportsByPattern {
    pub pattern: String,
    pub count: usize,
    pub samples: Vec<IgnoredUnresolvedImportSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct IgnoredUnresolvedImportSample {
    pub specifier: String,
    pub importer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceAliasIgnoreWarning {
    pub pattern: String,
    pub alias: String,
    pub source: String,
    pub fix_blocking: bool,
    pub message: String,
}

#[derive(Debug, Clone)]
struct AliasMapping {
    key: String,
    values: Vec<String>,
    source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportExplanation {
    pub exporting_file: String,
    pub symbol: String,
    pub file_reachable: bool,
    pub importers_considered: Vec<ExportImportExplanation>,
    pub importers_skipped: Vec<SkippedImporterExplanation>,
    pub ignored_unresolved_imports_that_might_have_pointed_at_this_file:
        Vec<IgnoredUnresolvedImportSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct ExportImportExplanation {
    pub importer: String,
    pub specifier: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct SkippedImporterExplanation {
    pub importer: String,
    pub specifier: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExplainExportRequest {
    pub file: String,
    pub symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainExportResult {
    pub exporting_file: String,
    pub symbol: String,
    pub status: ExplainExportStatus,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<ExportExplanation>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExplainExportStatus {
    Alive,
    Dead,
    FileUnused,
    FileNotFound,
    SymbolNotExported,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

pub fn analyze_path(
    cwd: &Path,
    config: &CodescytheConfig,
    options: AnalysisOptions,
) -> Result<Analysis> {
    let cwd = absolute_normalize_path(cwd)?;
    if !cwd.exists() {
        anyhow::bail!("analysis root does not exist: {}", cwd.display());
    }
    let project_files = discover_project_files(&cwd, config)?;
    let entry_files = discover_entry_files(&cwd, config, &project_files)?;
    let test_file_indexes = discover_test_file_indexes(&cwd, config, &project_files)?;
    let entry_set = entry_files.iter().cloned().collect::<HashSet<_>>();
    let total_files = project_files.len();

    let index_by_path = project_files
        .iter()
        .enumerate()
        .map(|(index, path)| (normalize_path(path), index))
        .collect::<HashMap<_, _>>();
    let module_resolver = ModuleResolver::new(&cwd, &project_files, config);
    let unresolved_policy = UnresolvedImportPolicy::new(config)?;
    let alias_mappings = source_alias_mappings(&cwd, config)?;
    let source_alias_ignore_warnings = source_alias_ignore_warnings(config, &alias_mappings)?;
    let mut files = FileCache::new(&cwd, project_files)?;

    let mut entry_indexes = HashSet::<usize>::new();
    let mut used_files = UsedFiles::new();
    let mut used_exports = UsedExports::new();
    let mut unresolved = UnresolvedImports::new();
    let mut ignored_unresolved_imports_by_pattern =
        BTreeMap::<String, IgnoredUnresolvedImportsByPattern>::new();
    let mut queue = VecDeque::<usize>::new();
    let mut queued_files = QueuedFiles::new();

    for index in &test_file_indexes {
        used_files.insert(*index);
    }

    for entry in &entry_set {
        if let Some(index) = index_by_path.get(&normalize_path(entry)) {
            entry_indexes.insert(*index);
            mark_used_file(
                *index,
                &test_file_indexes,
                &mut used_files,
                &mut queue,
                &mut queued_files,
            );
        }
    }

    while !queue.is_empty() {
        let batch = queue.drain(..).collect::<Vec<_>>();
        queued_files.clear();
        files.parse_many(&batch)?;

        for index in batch {
            let file = files.get(index)?.clone();
            let public_entry = entry_indexes.contains(&index) && !config.include_entry_exports;

            for import in &file.imports {
                match module_resolver.resolve(&file, &import.source)? {
                    ImportResolution::Project(target) => {
                        if let Some(name) = &import.imported {
                            mark_used_export(
                                target,
                                name.clone(),
                                &mut used_files,
                                &mut used_exports,
                                &mut queue,
                                &mut queued_files,
                                &test_file_indexes,
                            );
                        } else {
                            mark_used_file(
                                target,
                                &test_file_indexes,
                                &mut used_files,
                                &mut queue,
                                &mut queued_files,
                            );
                        }
                    }
                    ImportResolution::Unresolved => {
                        unresolved_policy.record(
                            &mut unresolved,
                            &mut ignored_unresolved_imports_by_pattern,
                            &file.relative,
                            &import.source,
                        )?;
                    }
                    ImportResolution::External => {}
                }
            }

            for source in &file.side_effect_imports {
                match module_resolver.resolve(&file, source)? {
                    ImportResolution::Project(target) => {
                        mark_used_file(
                            target,
                            &test_file_indexes,
                            &mut used_files,
                            &mut queue,
                            &mut queued_files,
                        );
                    }
                    ImportResolution::Unresolved => {
                        unresolved_policy.record(
                            &mut unresolved,
                            &mut ignored_unresolved_imports_by_pattern,
                            &file.relative,
                            source,
                        )?;
                    }
                    ImportResolution::External => {}
                }
            }

            for source in &file.dynamic_imports {
                mark_all_exports(
                    &file,
                    source,
                    &mut files,
                    &module_resolver,
                    &mut used_files,
                    &mut used_exports,
                    &mut queue,
                    &mut queued_files,
                    &mut unresolved,
                    &mut ignored_unresolved_imports_by_pattern,
                    &unresolved_policy,
                    &test_file_indexes,
                )?;
            }

            for pattern in &file.glob_imports {
                mark_glob_import(
                    &file,
                    pattern,
                    &mut files,
                    &mut used_files,
                    &mut used_exports,
                    &mut queue,
                    &mut queued_files,
                    &test_file_indexes,
                )?;
            }

            for (local, member) in &file.member_uses {
                if let Some(source) = file.namespace_imports.get(local) {
                    mark_member_import(
                        &file,
                        source,
                        member,
                        &mut files,
                        &module_resolver,
                        &mut used_files,
                        &mut used_exports,
                        &mut queue,
                        &mut queued_files,
                        &mut unresolved,
                        &mut ignored_unresolved_imports_by_pattern,
                        &unresolved_policy,
                        &file.relative,
                        &test_file_indexes,
                    )?;
                }

                if let Some(named) = file.named_imports.get(local) {
                    if let ImportResolution::Project(target) =
                        module_resolver.resolve(&file, &named.source)?
                    {
                        let namespace_source = files
                            .get(target)?
                            .exports
                            .get(&named.imported)
                            .and_then(|export| export.namespace_source.clone());
                        if let Some(namespace_source) = namespace_source {
                            let target_file = files.get(target)?.clone();
                            mark_member_import(
                                &target_file,
                                &namespace_source,
                                member,
                                &mut files,
                                &module_resolver,
                                &mut used_files,
                                &mut used_exports,
                                &mut queue,
                                &mut queued_files,
                                &mut unresolved,
                                &mut ignored_unresolved_imports_by_pattern,
                                &unresolved_policy,
                                &file.relative,
                                &test_file_indexes,
                            )?;
                        }
                    }
                }
            }

            for export in file.exports.values() {
                mark_reexport_source_file(
                    &file,
                    export,
                    &module_resolver,
                    &mut used_files,
                    &mut queue,
                    &mut queued_files,
                    &mut unresolved,
                    &mut ignored_unresolved_imports_by_pattern,
                    &unresolved_policy,
                    &test_file_indexes,
                )?;
            }
            for source in &file.reexport_all {
                mark_source_file(
                    &file,
                    source,
                    &module_resolver,
                    &mut used_files,
                    &mut queue,
                    &mut queued_files,
                    &mut unresolved,
                    &mut ignored_unresolved_imports_by_pattern,
                    &unresolved_policy,
                    &test_file_indexes,
                )?;
            }

            if public_entry {
                for export in file.exports.values() {
                    mark_reexport(
                        &file,
                        export,
                        &module_resolver,
                        &mut used_files,
                        &mut used_exports,
                        &mut queue,
                        &mut queued_files,
                        &mut unresolved,
                        &mut ignored_unresolved_imports_by_pattern,
                        &unresolved_policy,
                        &test_file_indexes,
                    )?;
                }
                for source in &file.reexport_all {
                    mark_all_exports(
                        &file,
                        source,
                        &mut files,
                        &module_resolver,
                        &mut used_files,
                        &mut used_exports,
                        &mut queue,
                        &mut queued_files,
                        &mut unresolved,
                        &mut ignored_unresolved_imports_by_pattern,
                        &unresolved_policy,
                        &test_file_indexes,
                    )?;
                }
            }

            let current_used_exports = used_exports.get(&index).cloned().unwrap_or_default();
            for export_name in current_used_exports {
                if let Some(export) = file.exports.get(&export_name) {
                    mark_reexport(
                        &file,
                        export,
                        &module_resolver,
                        &mut used_files,
                        &mut used_exports,
                        &mut queue,
                        &mut queued_files,
                        &mut unresolved,
                        &mut ignored_unresolved_imports_by_pattern,
                        &unresolved_policy,
                        &test_file_indexes,
                    )?;
                }
            }
        }
    }

    let mut issues = Issues::default();
    let mut unused_file_indexes = HashSet::<usize>::new();

    for index in 0..total_files {
        let relative = files.relative(index);
        let is_entry = entry_indexes.contains(&index);
        let is_used = used_files.contains(&index);
        let is_test = test_file_indexes.contains(&index);

        if !is_used && !is_entry && !is_test {
            issues.files.insert(
                relative.clone(),
                FileIssue {
                    path: relative.clone(),
                },
            );
            unused_file_indexes.insert(index);
            if !options.include_unreachable_exports {
                continue;
            }
        }

        if is_test {
            continue;
        }

        if is_entry && !config.include_entry_exports {
            continue;
        }

        let file = files.get(index)?;
        let used = used_exports.get(&index);
        for (name, export) in &file.exports {
            let used_by_import = used.is_some_and(|exports| exports.contains(name));
            let used_in_file = config.ignore_exports_used_in_file
                && export
                    .local_name
                    .as_ref()
                    .is_some_and(|local| file.local_references.contains(local));
            if !used_by_import && !used_in_file {
                issues
                    .exports
                    .entry(file.relative.clone())
                    .or_default()
                    .insert(
                        name.clone(),
                        SymbolIssue {
                            symbol: name.clone(),
                            kind: export.kind,
                            line: export.line,
                            col: export.col,
                            explanation: None,
                            span: (export.remove_span.start, export.remove_span.end),
                        },
                    );
            }
        }
    }

    let live_test_support_files = discover_live_test_support_files(
        &mut files,
        &module_resolver,
        &test_file_indexes,
        &unused_file_indexes,
        &used_files,
    )?;
    for index in &live_test_support_files {
        let relative = files.relative(*index);
        issues.files.remove(&relative);
        issues.exports.remove(&relative);
        unused_file_indexes.remove(index);
    }

    let removable_test_files = discover_removable_test_files(
        &mut files,
        &module_resolver,
        &test_file_indexes,
        &unused_file_indexes,
        &issues.exports,
    )?;
    for index in removable_test_files {
        let relative = files.relative(index);
        issues.files.insert(
            relative.clone(),
            FileIssue {
                path: relative.clone(),
            },
        );
    }

    let mut effective_used_files = used_files.clone();
    effective_used_files.extend(live_test_support_files.iter().copied());

    let export_usage = if options.verbose || options.explain_export.is_some() {
        Some(build_export_usage_index(
            &mut files,
            &module_resolver,
            &effective_used_files,
            &entry_indexes,
            &test_file_indexes,
        )?)
    } else {
        None
    };

    if options.verbose {
        if let Some(export_usage) = &export_usage {
            add_export_explanations(
                &mut issues,
                &files,
                &effective_used_files,
                export_usage,
                &alias_mappings,
                &ignored_unresolved_imports_by_pattern,
            );
        }
    }

    let explain_export = if let Some(request) = &options.explain_export {
        Some(explain_requested_export(
            request,
            &mut files,
            &issues,
            &effective_used_files,
            export_usage.as_ref(),
            &alias_mappings,
            &ignored_unresolved_imports_by_pattern,
        )?)
    } else {
        None
    };

    issues.unresolved = unresolved
        .into_iter()
        .map(|(file, imports)| {
            let mut imports = imports.into_iter().collect::<Vec<_>>();
            imports.sort();
            (file, imports)
        })
        .collect();

    let counters = Counters {
        files: issues.files.len(),
        exports: issues.exports.values().map(BTreeMap::len).sum(),
        unresolved: issues.unresolved.values().map(Vec::len).sum(),
        ignored_unresolved: ignored_unresolved_imports_by_pattern
            .values()
            .map(|ignored| ignored.count)
            .sum(),
        processed: total_files,
        total: total_files,
    };

    let summary = options.verbose.then(|| AnalysisSummary {
        version: env!("CARGO_PKG_VERSION").to_string(),
        config_path: options
            .config_path
            .as_ref()
            .map(|path| path.display().to_string()),
        project_count: total_files,
        entry_count: entry_indexes.len(),
        ignored_unresolved_count: counters.ignored_unresolved,
        ignored_unresolved_patterns: ignored_unresolved_imports_by_pattern
            .keys()
            .cloned()
            .collect(),
        package_import_keys: package_import_keys(&cwd).unwrap_or_default(),
        configured_alias_keys: config.aliases.keys().cloned().collect(),
    });

    let ignored_unresolved_imports_by_pattern =
        if options.verbose || options.retain_ignored_unresolved {
            ignored_unresolved_imports_by_pattern
        } else {
            BTreeMap::new()
        };

    Ok(Analysis {
        issues,
        counters,
        summary,
        ignored_unresolved_imports_by_pattern,
        source_alias_ignore_warnings,
        explain_export,
    })
}
