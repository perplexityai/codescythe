use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    env, fs,
    path::{Component, Path, PathBuf},
    thread,
};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use oxc::ast_visit::{Visit, walk};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::{Parser, ParserReturn};
use oxc_resolver::{AliasValue, ResolveError, ResolveOptions, Resolver, TsconfigDiscovery};
use oxc_span::{GetSpan, SourceType, Span};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use walkdir::{DirEntry, WalkDir};

use crate::{CodescytheConfig, UnresolvedImportsMode};

const PARSE_THREADS_ENV: &str = "CODESCYTHE_PARSE_THREADS";
const RAYON_THREADS_ENV: &str = "RAYON_NUM_THREADS";

type UsedFiles = HashSet<usize>;
type UsedExports = HashMap<usize, HashSet<String>>;
type UnresolvedImports = HashMap<String, HashSet<String>>;

#[derive(Debug, Clone, Copy, Default)]
pub struct AnalysisOptions {
    pub include_unreachable_exports: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub issues: Issues,
    pub counters: Counters,
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
    pub processed: usize,
    pub total: usize,
}

pub fn analyze_path(
    cwd: &Path,
    config: &CodescytheConfig,
    options: AnalysisOptions,
) -> Result<Analysis> {
    let cwd = normalize_path(cwd);
    if !cwd.exists() {
        anyhow::bail!("analysis root does not exist: {}", cwd.display());
    }
    let project_files = discover_project_files(&cwd, config)?;
    let entry_files = discover_entry_files(&cwd, config, &project_files)?;
    let entry_set = entry_files.iter().cloned().collect::<HashSet<_>>();

    let files = parse_project_files(&cwd, &project_files)?;

    let index_by_path = files
        .iter()
        .enumerate()
        .map(|(index, file)| (file.path.clone(), index))
        .collect::<HashMap<_, _>>();
    let module_resolver = ModuleResolver::new(&cwd, &files, config);
    let unresolved_policy = UnresolvedImportPolicy::new(config)?;

    let mut entry_indexes = HashSet::<usize>::new();
    let mut used_files = UsedFiles::new();
    let mut used_exports = UsedExports::new();
    let mut unresolved = UnresolvedImports::new();
    let mut queue = VecDeque::<usize>::new();

    for entry in &entry_set {
        if let Some(index) = index_by_path.get(entry) {
            entry_indexes.insert(*index);
            if used_files.insert(*index) {
                queue.push_back(*index);
            }
        }
    }

    while let Some(index) = queue.pop_front() {
        let file = &files[index];
        let public_entry = entry_indexes.contains(&index) && !config.include_entry_exports;

        for import in &file.imports {
            match module_resolver.resolve(file, &import.source)? {
                ImportResolution::Project(target) => {
                    if used_files.insert(target) {
                        queue.push_back(target);
                    }
                    if let Some(name) = &import.imported {
                        used_exports.entry(target).or_default().insert(name.clone());
                    }
                }
                ImportResolution::Unresolved => {
                    unresolved_policy.record(&mut unresolved, &file.relative, &import.source)?;
                }
                ImportResolution::External => {}
            }
        }

        for source in &file.side_effect_imports {
            match module_resolver.resolve(file, source)? {
                ImportResolution::Project(target) => {
                    if used_files.insert(target) {
                        queue.push_back(target);
                    }
                }
                ImportResolution::Unresolved => {
                    unresolved_policy.record(&mut unresolved, &file.relative, source)?;
                }
                ImportResolution::External => {}
            }
        }

        for (local, member) in &file.member_uses {
            if let Some(source) = file.namespace_imports.get(local) {
                mark_member_import(
                    file,
                    source,
                    member,
                    &files,
                    &module_resolver,
                    &mut used_files,
                    &mut used_exports,
                    &mut queue,
                    &mut unresolved,
                    &unresolved_policy,
                    &file.relative,
                )?;
            }

            if let Some(named) = file.named_imports.get(local) {
                if let ImportResolution::Project(target) =
                    module_resolver.resolve(file, &named.source)?
                {
                    if let Some(export) = files[target].exports.get(&named.imported) {
                        if let Some(namespace_source) = &export.namespace_source {
                            mark_member_import(
                                &files[target],
                                namespace_source,
                                member,
                                &files,
                                &module_resolver,
                                &mut used_files,
                                &mut used_exports,
                                &mut queue,
                                &mut unresolved,
                                &unresolved_policy,
                                &file.relative,
                            )?;
                        }
                    }
                }
            }
        }

        if public_entry {
            for export in file.exports.values() {
                mark_reexport(
                    file,
                    export,
                    &module_resolver,
                    &mut used_files,
                    &mut used_exports,
                    &mut queue,
                    &mut unresolved,
                    &unresolved_policy,
                )?;
            }
            for source in &file.reexport_all {
                mark_all_exports(
                    file,
                    source,
                    &files,
                    &module_resolver,
                    &mut used_files,
                    &mut used_exports,
                    &mut queue,
                    &mut unresolved,
                    &unresolved_policy,
                )?;
            }
        }

        let current_used_exports = used_exports.get(&index).cloned().unwrap_or_default();
        for export_name in current_used_exports {
            if let Some(export) = file.exports.get(&export_name) {
                mark_reexport(
                    file,
                    export,
                    &module_resolver,
                    &mut used_files,
                    &mut used_exports,
                    &mut queue,
                    &mut unresolved,
                    &unresolved_policy,
                )?;
            }
        }
    }

    let mut issues = Issues::default();

    for (index, file) in files.iter().enumerate() {
        let is_entry = entry_indexes.contains(&index);
        let is_used = used_files.contains(&index);

        if !is_used && !is_entry {
            issues.files.insert(
                file.relative.clone(),
                FileIssue {
                    path: file.relative.clone(),
                },
            );
            if !options.include_unreachable_exports {
                continue;
            }
        }

        if is_entry && !config.include_entry_exports {
            continue;
        }

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
                            span: (export.remove_span.start, export.remove_span.end),
                        },
                    );
            }
        }
    }

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
        processed: files.len(),
        total: project_files.len(),
    };

    Ok(Analysis { issues, counters })
}

fn discover_project_files(cwd: &Path, config: &CodescytheConfig) -> Result<Vec<PathBuf>> {
    let include = build_glob_set(&config.project)?;
    let ignore = build_glob_set(&config.ignore)?;
    let mut files = Vec::new();

    for entry in WalkDir::new(cwd)
        .follow_links(true)
        .into_iter()
        .filter_entry(|entry| should_enter(entry))
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = relative_path(cwd, entry.path());
        if include.is_match(&relative) && !ignore.is_match(&relative) {
            files.push(entry.path().to_path_buf());
        }
    }

    files.sort();
    Ok(files)
}

fn discover_entry_files(
    cwd: &Path,
    config: &CodescytheConfig,
    project_files: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    if config.entry.is_empty() {
        let inferred = infer_entry_files(cwd)?;
        return Ok(inferred
            .into_iter()
            .filter(|path| project_files.contains(path))
            .collect());
    }

    let entry_globs = build_glob_set(&config.entry)?;
    let mut entries = BTreeSet::<PathBuf>::new();
    for pattern in &config.entry {
        if !has_glob_meta(pattern) {
            let path = normalize_path(&cwd.join(pattern));
            if path.exists() {
                entries.insert(path);
            }
        }
    }
    for file in project_files {
        if entry_globs.is_match(relative_path(cwd, file)) {
            entries.insert(file.clone());
        }
    }
    Ok(entries.into_iter().collect())
}

fn infer_entry_files(cwd: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = BTreeSet::<PathBuf>::new();
    for candidate in [
        "src/index.ts",
        "src/index.tsx",
        "src/index.js",
        "index.ts",
        "index.tsx",
        "index.js",
    ] {
        let path = cwd.join(candidate);
        if path.exists() {
            entries.insert(normalize_path(&path));
        }
    }

    let package_json = cwd.join("package.json");
    if package_json.exists() {
        let value = serde_json::from_str::<serde_json::Value>(&fs::read_to_string(package_json)?)?;
        for field in ["main", "module", "types"] {
            if let Some(path) = value.get(field).and_then(|value| value.as_str()) {
                let path = cwd.join(path);
                if path.exists() {
                    entries.insert(normalize_path(&path));
                }
            }
        }
        if let Some(bin) = value.get("bin") {
            match bin {
                serde_json::Value::String(path) => {
                    let path = cwd.join(path);
                    if path.exists() {
                        entries.insert(normalize_path(&path));
                    }
                }
                serde_json::Value::Object(map) => {
                    for path in map.values().filter_map(|value| value.as_str()) {
                        let path = cwd.join(path);
                        if path.exists() {
                            entries.insert(normalize_path(&path));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(entries.into_iter().collect())
}

fn build_glob_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder
            .add(Glob::new(pattern).with_context(|| format!("invalid glob pattern {pattern:?}"))?);
    }
    Ok(builder.build()?)
}

fn should_enter(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return true;
    }
    !matches!(
        entry.file_name().to_str(),
        Some(".git" | "node_modules" | "target" | "dist" | "build" | "coverage")
    ) && !entry.file_name().to_string_lossy().starts_with("bazel-")
}

fn parse_file(cwd: &Path, path: &Path) -> Result<FileData> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read source file {}", path.display()))?;
    let source_type = SourceType::from_path(path)
        .with_context(|| format!("unsupported source extension for {}", path.display()))?;
    let allocator = Allocator::default();
    let ParserReturn {
        program, errors, ..
    } = Parser::new(&allocator, &source, source_type).parse();

    if !errors.is_empty() {
        let rendered = errors
            .iter()
            .map(|error| format!("{error:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!("failed to parse {}:\n{}", path.display(), rendered);
    }

    let mut visitor = FileVisitor::new(cwd, path);
    visitor.visit_program(&program);
    let mut file = visitor.finish();
    for export in file.exports.values_mut() {
        (export.line, export.col) = line_col(&source, export.name_span.start);
    }
    Ok(file)
}

fn parse_project_files(cwd: &Path, project_files: &[PathBuf]) -> Result<Vec<FileData>> {
    let threads = parse_thread_count();
    if threads <= 1 {
        return project_files
            .iter()
            .map(|path| parse_file(cwd, path))
            .collect();
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .context("failed to build parse thread pool")?
        .install(|| {
            project_files
                .par_iter()
                .map(|path| parse_file(cwd, path))
                .collect()
        })
}

fn parse_thread_count() -> usize {
    if let Some(threads) = env_thread_count(PARSE_THREADS_ENV) {
        return threads;
    }
    if let Some(threads) = env_thread_count(RAYON_THREADS_ENV) {
        return threads;
    }

    thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
}

fn env_thread_count(name: &str) -> Option<usize> {
    env::var(name)
        .ok()?
        .parse::<usize>()
        .ok()
        .map(|count| count.max(1))
}

struct ModuleResolver {
    resolver: Resolver,
    index_by_path: HashMap<PathBuf, usize>,
}

enum ImportResolution {
    Project(usize),
    External,
    Unresolved,
}

impl ModuleResolver {
    fn new(cwd: &Path, files: &[FileData], config: &CodescytheConfig) -> Self {
        let resolver = Resolver::new(ResolveOptions {
            cwd: Some(cwd.to_path_buf()),
            tsconfig: Some(TsconfigDiscovery::Auto),
            alias: config_aliases(cwd, config),
            condition_names: vec!["node".into(), "import".into()],
            extensions: vec![
                ".ts".into(),
                ".tsx".into(),
                ".mts".into(),
                ".cts".into(),
                ".js".into(),
                ".jsx".into(),
                ".mjs".into(),
                ".cjs".into(),
                ".json".into(),
                ".node".into(),
            ],
            extension_alias: vec![
                (
                    ".js".into(),
                    vec![".ts".into(), ".tsx".into(), ".js".into(), ".jsx".into()],
                ),
                (".jsx".into(), vec![".tsx".into(), ".jsx".into()]),
                (".mjs".into(), vec![".mts".into(), ".mjs".into()]),
                (".cjs".into(), vec![".cts".into(), ".cjs".into()]),
            ],
            symlinks: false,
            node_path: false,
            builtin_modules: true,
            ..ResolveOptions::default()
        });
        let index_by_path = files
            .iter()
            .enumerate()
            .map(|(index, file)| (normalize_path(&file.path), index))
            .collect::<HashMap<_, _>>();

        Self {
            resolver,
            index_by_path,
        }
    }

    fn resolve(&self, from: &FileData, specifier: &str) -> Result<ImportResolution> {
        match self.resolver.resolve_file(&from.path, specifier) {
            Ok(resolution) => {
                let path = normalize_path(resolution.path());
                Ok(self
                    .index_by_path
                    .get(&path)
                    .copied()
                    .map_or(ImportResolution::External, ImportResolution::Project))
            }
            Err(ResolveError::Builtin { .. } | ResolveError::Ignored(_)) => {
                Ok(ImportResolution::External)
            }
            Err(error) if is_resolution_miss(&error) => {
                Ok(if should_report_unresolved(specifier, &error) {
                    ImportResolution::Unresolved
                } else {
                    ImportResolution::External
                })
            }
            Err(error) => {
                anyhow::bail!(
                    "failed to resolve import {specifier:?} from {}: {error}",
                    from.relative
                )
            }
        }
    }
}

fn config_aliases(cwd: &Path, config: &CodescytheConfig) -> Vec<(String, Vec<AliasValue>)> {
    config
        .aliases
        .iter()
        .map(|(key, values)| {
            (
                key.clone(),
                values
                    .iter()
                    .map(|value| AliasValue::Path(config_alias_value(cwd, value)))
                    .collect(),
            )
        })
        .collect()
}

fn config_alias_value(cwd: &Path, value: &str) -> String {
    if is_relative_alias_path(value) {
        return normalize_path(&cwd.join(value))
            .to_string_lossy()
            .replace('\\', "/");
    }
    value.to_string()
}

fn is_relative_alias_path(value: &str) -> bool {
    value == "." || value == ".." || value.starts_with("./") || value.starts_with("../")
}

struct UnresolvedImportPolicy {
    mode: UnresolvedImportsMode,
    ignore: GlobSet,
}

impl UnresolvedImportPolicy {
    fn new(config: &CodescytheConfig) -> Result<Self> {
        Ok(Self {
            mode: config.unresolved_imports.mode,
            ignore: build_glob_set(&config.unresolved_imports.ignore)?,
        })
    }

    fn record(
        &self,
        unresolved: &mut UnresolvedImports,
        importer: &str,
        specifier: &str,
    ) -> Result<()> {
        if self.ignore.is_match(specifier) {
            return Ok(());
        }

        match self.mode {
            UnresolvedImportsMode::Report => {
                unresolved
                    .entry(importer.to_string())
                    .or_default()
                    .insert(specifier.to_string());
                Ok(())
            }
            UnresolvedImportsMode::Ignore => Ok(()),
            UnresolvedImportsMode::Error => {
                anyhow::bail!("unresolved import {specifier:?} from {importer}")
            }
        }
    }
}

fn is_resolution_miss(error: &ResolveError) -> bool {
    matches!(
        error,
        ResolveError::NotFound(_)
            | ResolveError::MatchedAliasNotFound(_, _)
            | ResolveError::ExtensionAlias(_, _, _)
            | ResolveError::PackageImportNotDefined(_, _)
            | ResolveError::PackagePathNotExported { .. }
            | ResolveError::InvalidModuleSpecifier(_, _)
            | ResolveError::Specifier(_)
    )
}

fn should_report_unresolved(specifier: &str, error: &ResolveError) -> bool {
    matches!(
        error,
        ResolveError::MatchedAliasNotFound(_, _) | ResolveError::PackageImportNotDefined(_, _)
    ) || specifier.starts_with('.')
        || specifier.starts_with('/')
        || specifier.starts_with('#')
        || specifier.starts_with("@/")
        || specifier.starts_with("~/")
}

fn mark_member_import(
    from_file: &FileData,
    source: &str,
    member: &str,
    files: &[FileData],
    resolver: &ModuleResolver,
    used_files: &mut UsedFiles,
    used_exports: &mut UsedExports,
    queue: &mut VecDeque<usize>,
    unresolved: &mut UnresolvedImports,
    unresolved_policy: &UnresolvedImportPolicy,
    importer_relative: &str,
) -> Result<()> {
    match resolver.resolve(from_file, source)? {
        ImportResolution::Project(target) => {
            if used_files.insert(target) {
                queue.push_back(target);
            }
            used_exports
                .entry(target)
                .or_default()
                .insert(member.to_string());
            if let Some(export) = files[target].exports.get(member) {
                if let Some(namespace_source) = &export.namespace_source {
                    mark_member_import(
                        &files[target],
                        namespace_source,
                        member,
                        files,
                        resolver,
                        used_files,
                        used_exports,
                        queue,
                        unresolved,
                        unresolved_policy,
                        importer_relative,
                    )?;
                }
            }
        }
        ImportResolution::Unresolved => {
            unresolved_policy.record(unresolved, importer_relative, source)?;
        }
        ImportResolution::External => {}
    }
    Ok(())
}

fn mark_reexport(
    file: &FileData,
    export: &ExportInfo,
    resolver: &ModuleResolver,
    used_files: &mut UsedFiles,
    used_exports: &mut UsedExports,
    queue: &mut VecDeque<usize>,
    unresolved: &mut UnresolvedImports,
    unresolved_policy: &UnresolvedImportPolicy,
) -> Result<()> {
    if let (Some(source), Some(name)) = (&export.reexport_source, &export.reexport_name) {
        match resolver.resolve(file, source)? {
            ImportResolution::Project(target) => {
                if used_files.insert(target) {
                    queue.push_back(target);
                }
                used_exports.entry(target).or_default().insert(name.clone());
            }
            ImportResolution::Unresolved => {
                unresolved_policy.record(unresolved, &file.relative, source)?;
            }
            ImportResolution::External => {}
        }
    }

    if let Some(source) = &export.namespace_source {
        match resolver.resolve(file, source)? {
            ImportResolution::Project(target) => {
                if used_files.insert(target) {
                    queue.push_back(target);
                }
            }
            ImportResolution::Unresolved => {
                unresolved_policy.record(unresolved, &file.relative, source)?;
            }
            ImportResolution::External => {}
        }
    }
    Ok(())
}

fn mark_all_exports(
    file: &FileData,
    source: &str,
    files: &[FileData],
    resolver: &ModuleResolver,
    used_files: &mut UsedFiles,
    used_exports: &mut UsedExports,
    queue: &mut VecDeque<usize>,
    unresolved: &mut UnresolvedImports,
    unresolved_policy: &UnresolvedImportPolicy,
) -> Result<()> {
    match resolver.resolve(file, source)? {
        ImportResolution::Project(target) => {
            if used_files.insert(target) {
                queue.push_back(target);
            }
            for name in files[target].exports.keys() {
                used_exports.entry(target).or_default().insert(name.clone());
            }
        }
        ImportResolution::Unresolved => {
            unresolved_policy.record(unresolved, &file.relative, source)?;
        }
        ImportResolution::External => {}
    }
    Ok(())
}

#[derive(Debug)]
struct FileData {
    path: PathBuf,
    relative: String,
    exports: BTreeMap<String, ExportInfo>,
    imports: Vec<ImportRecord>,
    side_effect_imports: Vec<String>,
    namespace_imports: BTreeMap<String, String>,
    named_imports: BTreeMap<String, NamedImport>,
    member_uses: Vec<(String, String)>,
    reexport_all: Vec<String>,
    local_references: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct ExportInfo {
    kind: ExportKind,
    local_name: Option<String>,
    name_span: Span,
    line: usize,
    col: usize,
    remove_span: Span,
    reexport_source: Option<String>,
    reexport_name: Option<String>,
    namespace_source: Option<String>,
}

#[derive(Debug)]
struct ImportRecord {
    source: String,
    imported: Option<String>,
}

#[derive(Debug)]
struct NamedImport {
    source: String,
    imported: String,
}

struct FileVisitor {
    path: PathBuf,
    relative: String,
    exports: BTreeMap<String, ExportInfo>,
    imports: Vec<ImportRecord>,
    side_effect_imports: Vec<String>,
    namespace_imports: BTreeMap<String, String>,
    named_imports: BTreeMap<String, NamedImport>,
    member_uses: Vec<(String, String)>,
    reexport_all: Vec<String>,
    local_references: BTreeSet<String>,
}

impl FileVisitor {
    fn new(cwd: &Path, path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            relative: relative_path(cwd, path),
            exports: BTreeMap::new(),
            imports: Vec::new(),
            side_effect_imports: Vec::new(),
            namespace_imports: BTreeMap::new(),
            named_imports: BTreeMap::new(),
            member_uses: Vec::new(),
            reexport_all: Vec::new(),
            local_references: BTreeSet::new(),
        }
    }

    fn finish(self) -> FileData {
        FileData {
            path: self.path,
            relative: self.relative,
            exports: self.exports,
            imports: self.imports,
            side_effect_imports: self.side_effect_imports,
            namespace_imports: self.namespace_imports,
            named_imports: self.named_imports,
            member_uses: self.member_uses,
            reexport_all: self.reexport_all,
            local_references: self.local_references,
        }
    }

    fn add_export(
        &mut self,
        name: String,
        kind: ExportKind,
        local_name: Option<String>,
        name_span: Span,
        remove_span: Span,
    ) {
        self.exports.insert(
            name,
            ExportInfo {
                kind,
                local_name,
                name_span,
                line: 0,
                col: 0,
                remove_span,
                reexport_source: None,
                reexport_name: None,
                namespace_source: None,
            },
        );
    }

    fn add_reexport(
        &mut self,
        exported: String,
        local: String,
        source: String,
        kind: ExportKind,
        name_span: Span,
        remove_span: Span,
    ) {
        self.exports.insert(
            exported,
            ExportInfo {
                kind,
                local_name: None,
                name_span,
                line: 0,
                col: 0,
                remove_span,
                reexport_source: Some(source),
                reexport_name: Some(local),
                namespace_source: None,
            },
        );
    }
}

impl<'a> Visit<'a> for FileVisitor {
    fn visit_import_declaration(&mut self, declaration: &ImportDeclaration<'a>) {
        let source = declaration.source.value.as_str().to_string();
        match &declaration.specifiers {
            Some(specifiers) => {
                for specifier in specifiers {
                    match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                            let imported = module_export_name(&specifier.imported);
                            self.imports.push(ImportRecord {
                                source: source.clone(),
                                imported: Some(imported.clone()),
                            });
                            self.named_imports.insert(
                                specifier.local.name.as_str().to_string(),
                                NamedImport {
                                    source: source.clone(),
                                    imported,
                                },
                            );
                        }
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                            self.imports.push(ImportRecord {
                                source: source.clone(),
                                imported: Some("default".to_string()),
                            });
                            self.named_imports.insert(
                                specifier.local.name.as_str().to_string(),
                                NamedImport {
                                    source: source.clone(),
                                    imported: "default".to_string(),
                                },
                            );
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                            self.side_effect_imports.push(source.clone());
                            self.namespace_imports
                                .insert(specifier.local.name.as_str().to_string(), source.clone());
                        }
                    }
                }
            }
            None => self.side_effect_imports.push(source),
        }
    }

    fn visit_export_named_declaration(&mut self, declaration: &ExportNamedDeclaration<'a>) {
        let declaration_kind = export_kind(declaration.export_kind);
        if let Some(source) = &declaration.source {
            let source = source.value.as_str().to_string();
            for specifier in &declaration.specifiers {
                self.add_reexport(
                    module_export_name(&specifier.exported),
                    module_export_name(&specifier.local),
                    source.clone(),
                    declaration_kind.max(export_kind(specifier.export_kind)),
                    specifier.exported.span(),
                    declaration.span,
                );
            }
        } else {
            if let Some(inner) = &declaration.declaration {
                self.add_declaration_exports(inner, declaration.span, declaration_kind);
            }
            for specifier in &declaration.specifiers {
                let exported = module_export_name(&specifier.exported);
                let local = module_export_name(&specifier.local);
                self.add_export(
                    exported,
                    declaration_kind.max(export_kind(specifier.export_kind)),
                    Some(local),
                    specifier.exported.span(),
                    declaration.span,
                );
            }
        }
        walk::walk_export_named_declaration(self, declaration);
    }

    fn visit_export_default_declaration(&mut self, declaration: &ExportDefaultDeclaration<'a>) {
        let local_name = match &declaration.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                function.id.as_ref().map(|id| id.name.as_str().to_string())
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                class.id.as_ref().map(|id| id.name.as_str().to_string())
            }
            _ => None,
        };
        self.add_export(
            "default".to_string(),
            ExportKind::Value,
            local_name,
            declaration.span,
            declaration.span,
        );
        walk::walk_export_default_declaration(self, declaration);
    }

    fn visit_export_all_declaration(&mut self, declaration: &ExportAllDeclaration<'a>) {
        let source = declaration.source.value.as_str().to_string();
        if let Some(exported) = &declaration.exported {
            let name = module_export_name(exported);
            self.exports.insert(
                name,
                ExportInfo {
                    kind: export_kind(declaration.export_kind),
                    local_name: None,
                    name_span: exported.span(),
                    line: 0,
                    col: 0,
                    remove_span: declaration.span,
                    reexport_source: None,
                    reexport_name: None,
                    namespace_source: Some(source),
                },
            );
        } else {
            self.reexport_all.push(source);
        }
    }

    fn visit_static_member_expression(&mut self, expression: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(object) = &expression.object {
            self.member_uses.push((
                object.name.as_str().to_string(),
                expression.property.name.as_str().to_string(),
            ));
        }
        walk::walk_static_member_expression(self, expression);
    }

    fn visit_variable_declarator(&mut self, declaration: &VariableDeclarator<'a>) {
        if let Some(init) = &declaration.init {
            if let Some(source) = import_source_from_expression(init) {
                let mut names = Vec::new();
                collect_binding_names(&declaration.id, &mut names);
                for name in names {
                    self.imports.push(ImportRecord {
                        source: source.clone(),
                        imported: Some(name),
                    });
                }
            }
        }
        walk::walk_variable_declarator(self, declaration);
    }

    fn visit_import_expression(&mut self, expression: &ImportExpression<'a>) {
        if let Expression::StringLiteral(source) = &expression.source {
            self.side_effect_imports
                .push(source.value.as_str().to_string());
        }
        walk::walk_import_expression(self, expression);
    }

    fn visit_identifier_reference(&mut self, identifier: &IdentifierReference<'a>) {
        self.local_references
            .insert(identifier.name.as_str().to_string());
    }
}

impl FileVisitor {
    fn add_declaration_exports(
        &mut self,
        declaration: &Declaration<'_>,
        remove_span: Span,
        default_kind: ExportKind,
    ) {
        match declaration {
            Declaration::VariableDeclaration(variable) => {
                for declarator in &variable.declarations {
                    let mut names = Vec::new();
                    collect_binding_names(&declarator.id, &mut names);
                    for name in names {
                        self.add_export(
                            name.clone(),
                            default_kind,
                            Some(name),
                            declarator.id.span(),
                            remove_span,
                        );
                    }
                }
            }
            Declaration::FunctionDeclaration(function) => {
                if let Some(id) = &function.id {
                    self.add_export(
                        id.name.as_str().to_string(),
                        ExportKind::Value,
                        Some(id.name.as_str().to_string()),
                        id.span,
                        remove_span,
                    );
                }
            }
            Declaration::ClassDeclaration(class) => {
                if let Some(id) = &class.id {
                    self.add_export(
                        id.name.as_str().to_string(),
                        ExportKind::Value,
                        Some(id.name.as_str().to_string()),
                        id.span,
                        remove_span,
                    );
                }
            }
            Declaration::TSTypeAliasDeclaration(alias) => {
                self.add_export(
                    alias.id.name.as_str().to_string(),
                    ExportKind::Type,
                    Some(alias.id.name.as_str().to_string()),
                    alias.id.span,
                    remove_span,
                );
            }
            Declaration::TSInterfaceDeclaration(interface) => {
                self.add_export(
                    interface.id.name.as_str().to_string(),
                    ExportKind::Type,
                    Some(interface.id.name.as_str().to_string()),
                    interface.id.span,
                    remove_span,
                );
            }
            Declaration::TSEnumDeclaration(enumeration) => {
                self.add_export(
                    enumeration.id.name.as_str().to_string(),
                    ExportKind::Type,
                    Some(enumeration.id.name.as_str().to_string()),
                    enumeration.id.span,
                    remove_span,
                );
            }
            Declaration::TSModuleDeclaration(module) => {
                if let Some(name) = ts_module_name(module) {
                    self.add_export(
                        name.clone(),
                        ExportKind::Type,
                        Some(name),
                        module.span,
                        remove_span,
                    );
                }
            }
            Declaration::TSGlobalDeclaration(_) | Declaration::TSImportEqualsDeclaration(_) => {}
        }
    }
}

fn collect_binding_names(pattern: &BindingPattern<'_>, names: &mut Vec<String>) {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => {
            names.push(identifier.name.as_str().to_string());
        }
        BindingPattern::ObjectPattern(pattern) => {
            for property in &pattern.properties {
                collect_binding_names(&property.value, names);
            }
            if let Some(rest) = &pattern.rest {
                collect_binding_names(&rest.argument, names);
            }
        }
        BindingPattern::ArrayPattern(pattern) => {
            for element in pattern.elements.iter().flatten() {
                collect_binding_names(element, names);
            }
            if let Some(rest) = &pattern.rest {
                collect_binding_names(&rest.argument, names);
            }
        }
        BindingPattern::AssignmentPattern(pattern) => {
            collect_binding_names(&pattern.left, names);
        }
    }
}

fn import_source_from_expression(expression: &Expression<'_>) -> Option<String> {
    match expression {
        Expression::ImportExpression(import) => match &import.source {
            Expression::StringLiteral(source) => Some(source.value.as_str().to_string()),
            _ => None,
        },
        Expression::AwaitExpression(await_expression) => {
            import_source_from_expression(&await_expression.argument)
        }
        Expression::ParenthesizedExpression(parenthesized) => {
            import_source_from_expression(&parenthesized.expression)
        }
        _ => None,
    }
}

fn module_export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(identifier) => identifier.name.as_str().to_string(),
        ModuleExportName::IdentifierReference(identifier) => identifier.name.as_str().to_string(),
        ModuleExportName::StringLiteral(literal) => literal.value.as_str().to_string(),
    }
}

fn ts_module_name(module: &oxc_ast::ast::TSModuleDeclaration<'_>) -> Option<String> {
    match &module.id {
        oxc_ast::ast::TSModuleDeclarationName::Identifier(identifier) => {
            Some(identifier.name.as_str().to_string())
        }
        oxc_ast::ast::TSModuleDeclarationName::StringLiteral(literal) => {
            Some(literal.value.as_str().to_string())
        }
    }
}

fn export_kind(kind: ImportOrExportKind) -> ExportKind {
    match kind {
        ImportOrExportKind::Type => ExportKind::Type,
        ImportOrExportKind::Value => ExportKind::Value,
    }
}

fn line_col(source: &str, offset: u32) -> (usize, usize) {
    let offset = offset as usize;
    let mut line = 1;
    let mut col = 1;
    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn relative_path(cwd: &Path, path: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn has_glob_meta(pattern: &str) -> bool {
    pattern
        .bytes()
        .any(|byte| matches!(byte, b'*' | b'?' | b'[' | b'{'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs};

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
}
