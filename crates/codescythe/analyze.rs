use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use oxc::ast_visit::{Visit, walk};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::{Parser, ParserReturn};
use oxc_span::{GetSpan, SourceType, Span};
use serde::{Deserialize, Serialize};
use walkdir::{DirEntry, WalkDir};

use crate::CodescytheConfig;

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
    let cwd = cwd
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", cwd.display()))?;
    let project_files = discover_project_files(&cwd, config)?;
    let entry_files = discover_entry_files(&cwd, config, &project_files)?;
    let entry_set = entry_files.iter().cloned().collect::<BTreeSet<_>>();

    let mut files = Vec::with_capacity(project_files.len());
    for path in &project_files {
        files.push(parse_file(&cwd, path)?);
    }

    let index_by_path = files
        .iter()
        .enumerate()
        .map(|(index, file)| (file.path.clone(), index))
        .collect::<BTreeMap<_, _>>();

    let mut used_files = BTreeSet::<usize>::new();
    let mut used_exports = BTreeMap::<usize, BTreeSet<String>>::new();
    let mut unresolved = BTreeMap::<String, BTreeSet<String>>::new();
    let mut queue = VecDeque::<usize>::new();

    for entry in &entry_set {
        if let Some(index) = index_by_path.get(entry) {
            if used_files.insert(*index) {
                queue.push_back(*index);
            }
        }
    }

    while let Some(index) = queue.pop_front() {
        let file = &files[index];
        let public_entry = entry_set.contains(&file.path) && !config.include_entry_exports;

        for import in &file.imports {
            match resolve_import(&cwd, &file.path, &import.source, &index_by_path) {
                Some(target) => {
                    if used_files.insert(target) {
                        queue.push_back(target);
                    }
                    if let Some(name) = &import.imported {
                        used_exports.entry(target).or_default().insert(name.clone());
                    }
                }
                None => {
                    unresolved
                        .entry(file.relative.clone())
                        .or_default()
                        .insert(import.source.clone());
                }
            }
        }

        for source in &file.side_effect_imports {
            match resolve_import(&cwd, &file.path, source, &index_by_path) {
                Some(target) => {
                    if used_files.insert(target) {
                        queue.push_back(target);
                    }
                }
                None => {
                    unresolved
                        .entry(file.relative.clone())
                        .or_default()
                        .insert(source.clone());
                }
            }
        }

        for (local, member) in &file.member_uses {
            if let Some(source) = file.namespace_imports.get(local) {
                mark_member_import(
                    &cwd,
                    &file.path,
                    source,
                    member,
                    &files,
                    &index_by_path,
                    &mut used_files,
                    &mut used_exports,
                    &mut queue,
                    &mut unresolved,
                    &file.relative,
                );
            }

            if let Some(named) = file.named_imports.get(local) {
                if let Some(target) =
                    resolve_import(&cwd, &file.path, &named.source, &index_by_path)
                {
                    if let Some(export) = files[target].exports.get(&named.imported) {
                        if let Some(namespace_source) = &export.namespace_source {
                            mark_member_import(
                                &cwd,
                                &files[target].path,
                                namespace_source,
                                member,
                                &files,
                                &index_by_path,
                                &mut used_files,
                                &mut used_exports,
                                &mut queue,
                                &mut unresolved,
                                &file.relative,
                            );
                        }
                    }
                }
            }
        }

        if public_entry {
            for export in file.exports.values() {
                mark_reexport(
                    &cwd,
                    file,
                    export,
                    &index_by_path,
                    &mut used_files,
                    &mut used_exports,
                    &mut queue,
                    &mut unresolved,
                );
            }
            for source in &file.reexport_all {
                mark_all_exports(
                    &cwd,
                    file,
                    source,
                    &files,
                    &index_by_path,
                    &mut used_files,
                    &mut used_exports,
                    &mut queue,
                    &mut unresolved,
                );
            }
        }

        let current_used_exports = used_exports.get(&index).cloned().unwrap_or_default();
        for export_name in current_used_exports {
            if let Some(export) = file.exports.get(&export_name) {
                mark_reexport(
                    &cwd,
                    file,
                    export,
                    &index_by_path,
                    &mut used_files,
                    &mut used_exports,
                    &mut queue,
                    &mut unresolved,
                );
            }
        }
    }

    let mut issues = Issues::default();

    for (index, file) in files.iter().enumerate() {
        let is_entry = entry_set.contains(&file.path);
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
                let (line, col) = line_col(&file.source, export.name_span.start);
                issues
                    .exports
                    .entry(file.relative.clone())
                    .or_default()
                    .insert(
                        name.clone(),
                        SymbolIssue {
                            symbol: name.clone(),
                            kind: export.kind,
                            line,
                            col,
                            span: (export.remove_span.start, export.remove_span.end),
                        },
                    );
            }
        }
    }

    issues.unresolved = unresolved
        .into_iter()
        .map(|(file, imports)| (file, imports.into_iter().collect()))
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
            let path = cwd
                .join(pattern)
                .canonicalize()
                .unwrap_or_else(|_| cwd.join(pattern));
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
            entries.insert(path.canonicalize()?);
        }
    }

    let package_json = cwd.join("package.json");
    if package_json.exists() {
        let value = serde_json::from_str::<serde_json::Value>(&fs::read_to_string(package_json)?)?;
        for field in ["main", "module", "types"] {
            if let Some(path) = value.get(field).and_then(|value| value.as_str()) {
                let path = cwd.join(path);
                if path.exists() {
                    entries.insert(path.canonicalize()?);
                }
            }
        }
        if let Some(bin) = value.get("bin") {
            match bin {
                serde_json::Value::String(path) => {
                    let path = cwd.join(path);
                    if path.exists() {
                        entries.insert(path.canonicalize()?);
                    }
                }
                serde_json::Value::Object(map) => {
                    for path in map.values().filter_map(|value| value.as_str()) {
                        let path = cwd.join(path);
                        if path.exists() {
                            entries.insert(path.canonicalize()?);
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

    let mut visitor = FileVisitor::new(cwd, path, source.clone());
    visitor.visit_program(&program);
    Ok(visitor.finish())
}

fn resolve_import(
    cwd: &Path,
    from: &Path,
    specifier: &str,
    index_by_path: &BTreeMap<PathBuf, usize>,
) -> Option<usize> {
    if !specifier.starts_with('.') {
        return None;
    }

    let base = from.parent()?.join(specifier);
    let mut candidates = Vec::new();
    candidates.push(base.clone());
    for ext in ["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"] {
        candidates.push(base.with_extension(ext));
    }
    for ext in ["ts", "tsx", "js", "jsx"] {
        candidates.push(base.join(format!("index.{ext}")));
    }

    candidates
        .into_iter()
        .filter_map(|candidate| normalize_existing(cwd, &candidate))
        .find_map(|candidate| index_by_path.get(&candidate).copied())
}

fn normalize_existing(cwd: &Path, path: &Path) -> Option<PathBuf> {
    if path.exists() {
        path.canonicalize().ok()
    } else {
        let normalized = cwd.join(path.strip_prefix(cwd).ok()?);
        if normalized.exists() {
            normalized.canonicalize().ok()
        } else {
            None
        }
    }
}

fn mark_member_import(
    cwd: &Path,
    from: &Path,
    source: &str,
    member: &str,
    files: &[FileData],
    index_by_path: &BTreeMap<PathBuf, usize>,
    used_files: &mut BTreeSet<usize>,
    used_exports: &mut BTreeMap<usize, BTreeSet<String>>,
    queue: &mut VecDeque<usize>,
    unresolved: &mut BTreeMap<String, BTreeSet<String>>,
    importer_relative: &str,
) {
    match resolve_import(cwd, from, source, index_by_path) {
        Some(target) => {
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
                        cwd,
                        &files[target].path,
                        namespace_source,
                        member,
                        files,
                        index_by_path,
                        used_files,
                        used_exports,
                        queue,
                        unresolved,
                        importer_relative,
                    );
                }
            }
        }
        None => {
            unresolved
                .entry(importer_relative.to_string())
                .or_default()
                .insert(source.to_string());
        }
    }
}

fn mark_reexport(
    cwd: &Path,
    file: &FileData,
    export: &ExportInfo,
    index_by_path: &BTreeMap<PathBuf, usize>,
    used_files: &mut BTreeSet<usize>,
    used_exports: &mut BTreeMap<usize, BTreeSet<String>>,
    queue: &mut VecDeque<usize>,
    unresolved: &mut BTreeMap<String, BTreeSet<String>>,
) {
    if let (Some(source), Some(name)) = (&export.reexport_source, &export.reexport_name) {
        match resolve_import(cwd, &file.path, source, index_by_path) {
            Some(target) => {
                if used_files.insert(target) {
                    queue.push_back(target);
                }
                used_exports.entry(target).or_default().insert(name.clone());
            }
            None => {
                unresolved
                    .entry(file.relative.clone())
                    .or_default()
                    .insert(source.clone());
            }
        }
    }

    if let Some(source) = &export.namespace_source {
        match resolve_import(cwd, &file.path, source, index_by_path) {
            Some(target) => {
                if used_files.insert(target) {
                    queue.push_back(target);
                }
            }
            None => {
                unresolved
                    .entry(file.relative.clone())
                    .or_default()
                    .insert(source.clone());
            }
        }
    }
}

fn mark_all_exports(
    cwd: &Path,
    file: &FileData,
    source: &str,
    files: &[FileData],
    index_by_path: &BTreeMap<PathBuf, usize>,
    used_files: &mut BTreeSet<usize>,
    used_exports: &mut BTreeMap<usize, BTreeSet<String>>,
    queue: &mut VecDeque<usize>,
    unresolved: &mut BTreeMap<String, BTreeSet<String>>,
) {
    match resolve_import(cwd, &file.path, source, index_by_path) {
        Some(target) => {
            if used_files.insert(target) {
                queue.push_back(target);
            }
            for name in files[target].exports.keys() {
                used_exports.entry(target).or_default().insert(name.clone());
            }
        }
        None => {
            unresolved
                .entry(file.relative.clone())
                .or_default()
                .insert(source.to_string());
        }
    }
}

#[derive(Debug)]
struct FileData {
    path: PathBuf,
    relative: String,
    source: String,
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
    source: String,
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
    fn new(cwd: &Path, path: &Path, source: String) -> Self {
        Self {
            path: path.to_path_buf(),
            relative: relative_path(cwd, path),
            source,
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
            source: self.source,
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
