mod discovery;
mod doctor;
mod explain;
mod graph;
mod parse;
mod profile;
mod query;
mod resolver;
mod util;

#[cfg(test)]
mod tests;

pub use doctor::doctor_config;
pub use explain::ignored_unresolved_patterns_for_file;
pub use query::{
    QueryEdge, QueryEdgeKind, QueryGraph, QueryKind, QueryNode, QueryNodeKind, QueryPath,
    QueryRequest, QueryResult, QuerySelector, QuerySelectorKind, QueryUnresolvedImport, query_path,
};
pub use resolver::{
    source_alias_fix_blocking_ignore_warnings_for_config, source_alias_ignore_warnings_for_config,
};

use discovery::{discover_entry_files, discover_project_files, discover_test_file_indexes};
use explain::{add_export_explanations, build_export_usage_index, explain_requested_export};
use graph::{
    discover_live_test_support_files, discover_removable_test_files, internal_export_target,
    mark_all_exports, mark_glob_import, mark_internal_exports_used_by_tests, mark_member_import,
    mark_reexport, mark_reexport_source_file, mark_source_file, mark_used_export, mark_used_file,
};
use parse::{ExportInfo, FileCache, FileData};
use profile::AnalysisProfile;
#[cfg(feature = "profiling")]
use profile::AnalysisProfileReport;
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
    env, fmt, fs, io,
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
use oxc_resolver::{
    AliasValue, FileMetadata, FileSystem, FileSystemOs, ResolveError, ResolveOptions,
    ResolverGeneric, TsconfigDiscovery,
};
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
type InternalTestUsages = BTreeMap<ExportUsageKey, BTreeSet<ExportImportExplanation>>;

const TEST_FILE_LEAF_REASON: ExplanationReasonCode = ExplanationReasonCode::TestFileLeaf;

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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub internal_exports_used_by_tests: Vec<InternalExportTestUsage>,
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
    #[serde(skip_serializing_if = "is_false")]
    pub internal: bool,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresolved_imports: Vec<UnresolvedImportExplanation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub internal_exports_used_by_tests: Vec<InternalExportTestUsage>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedImportExplanation {
    pub importer: String,
    pub specifier: String,
    pub resolver_error: String,
    pub matched_aliases: Vec<UnresolvedImportMatchedAlias>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedImportMatchedAlias {
    pub source: String,
    pub key: String,
    pub target: String,
    pub expanded_target: String,
    pub candidate_files: Vec<UnresolvedImportCandidateFile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedImportCandidateFile {
    pub path: String,
    pub exists: bool,
    pub in_project: bool,
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
    #[serde(skip_serializing_if = "is_false")]
    pub internal: bool,
    pub file_reachable: bool,
    pub importers_considered: Vec<ExportImportExplanation>,
    pub importers_skipped: Vec<SkippedImporterExplanation>,
    pub ignored_unresolved_imports_that_might_have_pointed_at_this_file:
        Vec<IgnoredUnresolvedImportSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct ExplanationReason {
    pub code: ExplanationReasonCode,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ExplanationReason {
    pub fn new(code: ExplanationReasonCode) -> Self {
        Self {
            code,
            description: code.description().to_string(),
            detail: None,
        }
    }

    pub fn with_detail(code: ExplanationReasonCode, detail: String) -> Self {
        Self {
            code,
            description: code.description().to_string(),
            detail: Some(detail),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum ExplanationReasonCode {
    NamedImport,
    NamespaceMemberAccess,
    ReExport,
    DynamicImportMarksAllExports,
    ExportStarMarksAllExports,
    TestImportOfInternalExport,
    TestNamespaceAccessOfInternalExport,
    TestDynamicImportOfInternalExport,
    TestImportMetaGlobOfInternalExport,
    TestExportStarImportOfInternalExport,
    TestFileLeaf,
    ImporterUnreachable,
    FileOutsideProject,
    FileUnparseable,
    SymbolNotExported,
    EntryPublicFileSemantics,
    InternalExportUsedByTests,
    ReachableImporters,
    NoReachableImporters,
    ExportingFileUnreachable,
}

impl ExplanationReasonCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NamedImport => "namedImport",
            Self::NamespaceMemberAccess => "namespaceMemberAccess",
            Self::ReExport => "reExport",
            Self::DynamicImportMarksAllExports => "dynamicImportMarksAllExports",
            Self::ExportStarMarksAllExports => "exportStarMarksAllExports",
            Self::TestImportOfInternalExport => "testImportOfInternalExport",
            Self::TestNamespaceAccessOfInternalExport => "testNamespaceAccessOfInternalExport",
            Self::TestDynamicImportOfInternalExport => "testDynamicImportOfInternalExport",
            Self::TestImportMetaGlobOfInternalExport => "testImportMetaGlobOfInternalExport",
            Self::TestExportStarImportOfInternalExport => "testExportStarImportOfInternalExport",
            Self::TestFileLeaf => "testFileLeaf",
            Self::ImporterUnreachable => "importerUnreachable",
            Self::FileOutsideProject => "fileOutsideProject",
            Self::FileUnparseable => "fileUnparseable",
            Self::SymbolNotExported => "symbolNotExported",
            Self::EntryPublicFileSemantics => "entryPublicFileSemantics",
            Self::InternalExportUsedByTests => "internalExportUsedByTests",
            Self::ReachableImporters => "reachableImporters",
            Self::NoReachableImporters => "noReachableImporters",
            Self::ExportingFileUnreachable => "exportingFileUnreachable",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::NamedImport => "named import",
            Self::NamespaceMemberAccess => "namespace member access",
            Self::ReExport => "re-export",
            Self::DynamicImportMarksAllExports => "dynamic import marks all exports",
            Self::ExportStarMarksAllExports => "export star marks all exports",
            Self::TestImportOfInternalExport => "test import of @internal export",
            Self::TestNamespaceAccessOfInternalExport => {
                "test namespace access of @internal export"
            }
            Self::TestDynamicImportOfInternalExport => "test dynamic import of @internal export",
            Self::TestImportMetaGlobOfInternalExport => "test import.meta.glob of @internal export",
            Self::TestExportStarImportOfInternalExport => {
                "test export star import of @internal export"
            }
            Self::TestFileLeaf => "test file leaf",
            Self::ImporterUnreachable => "importer unreachable",
            Self::FileOutsideProject => "file is outside the analyzed project set",
            Self::FileUnparseable => "file is unreachable and could not be parsed",
            Self::SymbolNotExported => "symbol is not exported by the requested file",
            Self::EntryPublicFileSemantics => "export is kept alive by entry/public-file semantics",
            Self::InternalExportUsedByTests => "export is marked @internal and used by tests",
            Self::ReachableImporters => "export is used by reachable importers",
            Self::NoReachableImporters => "export is not used by reachable importers",
            Self::ExportingFileUnreachable => "exporting file is unreachable",
        }
    }
}

impl fmt::Display for ExplanationReasonCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct ExportImportExplanation {
    pub importer: String,
    pub specifier: String,
    pub reason: ExplanationReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InternalExportTestUsage {
    pub exporting_file: String,
    pub symbol: String,
    pub test_importers: Vec<ExportImportExplanation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct SkippedImporterExplanation {
    pub importer: String,
    pub specifier: String,
    pub reason: ExplanationReason,
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
    pub reason: ExplanationReason,
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

fn is_false(value: &bool) -> bool {
    !*value
}

fn process_reachable_queue(
    profile: &mut AnalysisProfile,
    config: &CodescytheConfig,
    entry_indexes: &HashSet<usize>,
    test_file_indexes: &TestFiles,
    module_resolver: &ModuleResolver,
    unresolved_policy: &UnresolvedImportPolicy,
    files: &mut FileCache,
    used_files: &mut UsedFiles,
    used_exports: &mut UsedExports,
    unresolved: &mut UnresolvedImports,
    ignored_unresolved_imports_by_pattern: &mut BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
    queue: &mut VecDeque<usize>,
    queued_files: &mut QueuedFiles,
) -> Result<()> {
    while !queue.is_empty() {
        let batch = queue.drain(..).collect::<Vec<_>>();
        profile.record_frontier(batch.len());
        queued_files.clear();
        let parse_started = profile.start();
        files.parse_many(&batch)?;
        profile.record_frontier_parse(parse_started);

        let inspect_started = profile.start();
        for index in batch {
            let file = files.get(index)?.clone();
            let public_entry = entry_indexes.contains(&index) && !config.include_entry_exports;

            let mut static_imports_by_source = BTreeMap::<&str, BTreeSet<Option<&str>>>::new();
            for import in &file.imports {
                static_imports_by_source
                    .entry(import.source.as_str())
                    .or_default()
                    .insert(import.imported.as_deref());
            }
            for source in &file.side_effect_imports {
                static_imports_by_source
                    .entry(source.as_str())
                    .or_default()
                    .insert(None);
            }

            for (source, imported_names) in static_imports_by_source {
                match module_resolver.resolve(&file, source)? {
                    ImportResolution::Project(target) => {
                        for imported in imported_names {
                            if let Some(name) = imported {
                                mark_used_export(
                                    target,
                                    name.to_string(),
                                    used_files,
                                    used_exports,
                                    queue,
                                    queued_files,
                                    test_file_indexes,
                                );
                            } else {
                                mark_used_file(
                                    target,
                                    test_file_indexes,
                                    used_files,
                                    queue,
                                    queued_files,
                                );
                            }
                        }
                    }
                    ImportResolution::Unresolved => {
                        unresolved_policy.record(
                            unresolved,
                            ignored_unresolved_imports_by_pattern,
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
                    files,
                    module_resolver,
                    used_files,
                    used_exports,
                    queue,
                    queued_files,
                    unresolved,
                    ignored_unresolved_imports_by_pattern,
                    unresolved_policy,
                    test_file_indexes,
                )?;
            }

            for pattern in &file.glob_imports {
                mark_glob_import(
                    &file,
                    pattern,
                    files,
                    used_files,
                    used_exports,
                    queue,
                    queued_files,
                    test_file_indexes,
                )?;
            }

            for (local, member) in &file.member_uses {
                if let Some(source) = file.namespace_imports.get(local) {
                    mark_member_import(
                        &file,
                        source,
                        member,
                        files,
                        module_resolver,
                        used_files,
                        used_exports,
                        queue,
                        queued_files,
                        unresolved,
                        ignored_unresolved_imports_by_pattern,
                        unresolved_policy,
                        &file.relative,
                        test_file_indexes,
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
                                files,
                                module_resolver,
                                used_files,
                                used_exports,
                                queue,
                                queued_files,
                                unresolved,
                                ignored_unresolved_imports_by_pattern,
                                unresolved_policy,
                                &file.relative,
                                test_file_indexes,
                            )?;
                        }
                    }
                }
            }

            for export in file.exports.values() {
                mark_reexport_source_file(
                    &file,
                    export,
                    module_resolver,
                    used_files,
                    queue,
                    queued_files,
                    unresolved,
                    ignored_unresolved_imports_by_pattern,
                    unresolved_policy,
                    test_file_indexes,
                )?;
            }
            for source in &file.reexport_all {
                mark_source_file(
                    &file,
                    source,
                    module_resolver,
                    used_files,
                    queue,
                    queued_files,
                    unresolved,
                    ignored_unresolved_imports_by_pattern,
                    unresolved_policy,
                    test_file_indexes,
                )?;
            }

            if public_entry {
                for export in file.exports.values() {
                    mark_reexport(
                        &file,
                        export,
                        module_resolver,
                        used_files,
                        used_exports,
                        queue,
                        queued_files,
                        unresolved,
                        ignored_unresolved_imports_by_pattern,
                        unresolved_policy,
                        test_file_indexes,
                    )?;
                }
                for source in &file.reexport_all {
                    mark_all_exports(
                        &file,
                        source,
                        files,
                        module_resolver,
                        used_files,
                        used_exports,
                        queue,
                        queued_files,
                        unresolved,
                        ignored_unresolved_imports_by_pattern,
                        unresolved_policy,
                        test_file_indexes,
                    )?;
                }
            }

            let current_used_exports = used_exports.get(&index).cloned().unwrap_or_default();
            for export_name in current_used_exports {
                if let Some(export) = file.exports.get(&export_name) {
                    mark_reexport(
                        &file,
                        export,
                        module_resolver,
                        used_files,
                        used_exports,
                        queue,
                        queued_files,
                        unresolved,
                        ignored_unresolved_imports_by_pattern,
                        unresolved_policy,
                        test_file_indexes,
                    )?;
                }
            }
        }
        profile.record_frontier_inspect(inspect_started);
    }
    Ok(())
}

fn internal_test_usages_report(
    files: &FileCache,
    usages: InternalTestUsages,
) -> Vec<InternalExportTestUsage> {
    usages
        .into_iter()
        .map(|((index, symbol), importers)| InternalExportTestUsage {
            exporting_file: files.relative(index),
            symbol,
            test_importers: importers.into_iter().collect(),
        })
        .collect()
}

pub fn analyze_path(
    cwd: &Path,
    config: &CodescytheConfig,
    options: AnalysisOptions,
) -> Result<Analysis> {
    let mut profile = AnalysisProfile::new();
    let cwd = absolute_normalize_path(cwd)?;
    if !cwd.exists() {
        anyhow::bail!("analysis root does not exist: {}", cwd.display());
    }
    let project_files = profile.time("discover project files", || {
        discover_project_files(&cwd, config)
    })?;
    let entry_files = profile.time("discover entry files", || {
        discover_entry_files(&cwd, config, &project_files)
    })?;
    let test_file_indexes = profile.time("classify test files", || {
        discover_test_file_indexes(&cwd, config, &project_files)
    })?;
    let entry_set = entry_files.iter().cloned().collect::<HashSet<_>>();
    let total_files = project_files.len();

    let (
        index_by_path,
        module_resolver,
        unresolved_policy,
        alias_mappings,
        source_alias_ignore_warnings,
        mut files,
    ) = profile.time("build indexes and resolver", || {
        let index_by_path = project_files
            .iter()
            .enumerate()
            .map(|(index, path)| (normalize_path(path), index))
            .collect::<HashMap<_, _>>();
        let module_resolver = ModuleResolver::new(&cwd, &project_files, config)?;
        let unresolved_policy = UnresolvedImportPolicy::new(config)?;
        let alias_mappings = source_alias_mappings(&cwd, config)?;
        let source_alias_ignore_warnings = source_alias_ignore_warnings(config, &alias_mappings)?;
        let files = FileCache::new(&cwd, project_files)?;
        Ok::<_, anyhow::Error>((
            index_by_path,
            module_resolver,
            unresolved_policy,
            alias_mappings,
            source_alias_ignore_warnings,
            files,
        ))
    })?;

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

    let graph_started = profile.start();
    process_reachable_queue(
        &mut profile,
        config,
        &entry_indexes,
        &test_file_indexes,
        &module_resolver,
        &unresolved_policy,
        &mut files,
        &mut used_files,
        &mut used_exports,
        &mut unresolved,
        &mut ignored_unresolved_imports_by_pattern,
        &mut queue,
        &mut queued_files,
    )?;

    let internal_test_usages = mark_internal_exports_used_by_tests(
        &mut files,
        &module_resolver,
        &test_file_indexes,
        &mut used_files,
        &mut used_exports,
        &mut queue,
        &mut queued_files,
    )?;

    process_reachable_queue(
        &mut profile,
        config,
        &entry_indexes,
        &test_file_indexes,
        &module_resolver,
        &unresolved_policy,
        &mut files,
        &mut used_files,
        &mut used_exports,
        &mut unresolved,
        &mut ignored_unresolved_imports_by_pattern,
        &mut queue,
        &mut queued_files,
    )?;
    profile.record("walk reachable graph", graph_started);

    let issue_started = profile.start();
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
                            internal: export.internal,
                            line: export.line,
                            col: export.col,
                            explanation: None,
                            span: (export.remove_span.start, export.remove_span.end),
                        },
                    );
            }
        }
    }
    profile.record("build unused file/export issues", issue_started);

    let live_test_support_files = profile.time("scan live test support", || {
        discover_live_test_support_files(
            &mut files,
            &module_resolver,
            &test_file_indexes,
            &unused_file_indexes,
            &used_files,
        )
    })?;
    for index in &live_test_support_files {
        let relative = files.relative(*index);
        issues.files.remove(&relative);
        issues.exports.remove(&relative);
        unused_file_indexes.remove(index);
    }

    let removable_test_files = profile.time("scan removable tests", || {
        discover_removable_test_files(
            &mut files,
            &module_resolver,
            &test_file_indexes,
            &unused_file_indexes,
            &issues.exports,
        )
    })?;
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
    let internal_exports_used_by_tests = internal_test_usages_report(&files, internal_test_usages);

    let export_usage = profile.time("build export usage explanations", || {
        if options.verbose || options.explain_export.is_some() {
            Ok(Some(build_export_usage_index(
                &mut files,
                &module_resolver,
                &effective_used_files,
                &entry_indexes,
                &test_file_indexes,
            )?))
        } else {
            Ok(None)
        }
    })?;

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

    let explain_export = profile.time("explain requested export", || {
        if let Some(request) = &options.explain_export {
            Ok(Some(explain_requested_export(
                request,
                &mut files,
                &issues,
                &effective_used_files,
                export_usage.as_ref(),
                &alias_mappings,
                &ignored_unresolved_imports_by_pattern,
            )?))
        } else {
            Ok(None)
        }
    })?;

    let finalize_started = profile.start();
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
        package_import_keys: package_import_keys(&cwd, config).unwrap_or_default(),
        configured_alias_keys: config.aliases.keys().cloned().collect(),
    });

    let ignored_unresolved_imports_by_pattern =
        if options.verbose || options.retain_ignored_unresolved {
            ignored_unresolved_imports_by_pattern
        } else {
            BTreeMap::new()
        };

    profile.record("finalize report", finalize_started);
    #[cfg(feature = "profiling")]
    profile.print(AnalysisProfileReport {
        project_files: total_files,
        entry_files: entry_indexes.len(),
        test_files: test_file_indexes.len(),
        parsed_files: files.parsed_count(),
        used_files: effective_used_files.len(),
        used_exports: used_exports.values().map(HashSet::len).sum(),
        issue_files: counters.files,
        issue_exports: counters.exports,
        unresolved: counters.unresolved,
        ignored_unresolved: counters.ignored_unresolved,
        resolver: module_resolver.profile_stats(),
    });

    Ok(Analysis {
        issues,
        counters,
        summary,
        ignored_unresolved_imports_by_pattern,
        source_alias_ignore_warnings,
        internal_exports_used_by_tests,
        explain_export,
    })
}
