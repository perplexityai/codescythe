use super::*;

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
    match file.exports.len() {
        0 => {}
        1 => {
            for export in file.exports.values_mut() {
                (export.line, export.col) = line_col(&source, export.name_span.start);
            }
        }
        _ => {
            let line_starts = line_starts(&source);
            for export in file.exports.values_mut() {
                (export.line, export.col) =
                    line_col_from_starts(&source, &line_starts, export.name_span.start);
            }
        }
    }
    Ok(file)
}

pub(super) struct FileCache {
    pub(super) cwd: PathBuf,
    pub(super) paths: Vec<PathBuf>,
    parsed: Vec<Option<FileData>>,
    parse_pool: Option<rayon::ThreadPool>,
}

impl FileCache {
    pub(super) fn new(cwd: &Path, paths: Vec<PathBuf>) -> Result<Self> {
        let mut parsed = Vec::with_capacity(paths.len());
        parsed.resize_with(paths.len(), || None);
        let threads = parse_thread_count();
        let parse_pool = if threads > 1 {
            Some(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .build()
                    .context("failed to build parse thread pool")?,
            )
        } else {
            None
        };
        Ok(Self {
            cwd: cwd.to_path_buf(),
            paths,
            parsed,
            parse_pool,
        })
    }

    pub(super) fn get(&mut self, index: usize) -> Result<&FileData> {
        if self.parsed[index].is_none() {
            self.parsed[index] = Some(parse_file(&self.cwd, &self.paths[index])?);
        }
        Ok(self.parsed[index]
            .as_ref()
            .expect("file should be parsed before returning"))
    }

    pub(super) fn try_get(&mut self, index: usize) -> std::result::Result<&FileData, String> {
        if self.parsed[index].is_none() {
            self.parsed[index] = Some(
                parse_file(&self.cwd, &self.paths[index]).map_err(|error| format!("{error:#}"))?,
            );
        }
        Ok(self.parsed[index]
            .as_ref()
            .expect("file should be parsed before returning"))
    }

    pub(super) fn parse_many(&mut self, indexes: &[usize]) -> Result<()> {
        let missing = indexes
            .iter()
            .copied()
            .filter(|index| self.parsed[*index].is_none())
            .collect::<Vec<_>>();
        let parsed = if let Some(pool) = &self.parse_pool {
            pool.install(|| {
                missing
                    .par_iter()
                    .map(|index| {
                        parse_file(&self.cwd, &self.paths[*index]).map(|file| (*index, file))
                    })
                    .collect::<Result<Vec<_>>>()
            })
        } else {
            missing
                .iter()
                .map(|index| parse_file(&self.cwd, &self.paths[*index]).map(|file| (*index, file)))
                .collect::<Result<Vec<_>>>()
        }?;

        for (index, file) in parsed {
            if self.parsed[index].is_none() {
                self.parsed[index] = Some(file);
            }
        }
        Ok(())
    }

    pub(super) fn relative(&self, index: usize) -> String {
        relative_path(&self.cwd, &self.paths[index])
    }

    pub(super) fn index_by_relative(&self, relative: &str) -> Option<usize> {
        self.paths
            .iter()
            .position(|path| relative_path(&self.cwd, path) == relative)
    }

    pub(super) fn matching_relative_glob(&self, pattern: &str) -> Result<Vec<usize>> {
        let glob = build_glob_set(&[pattern.to_string()])?;
        Ok(self
            .paths
            .iter()
            .enumerate()
            .filter_map(|(index, path)| {
                glob.is_match(relative_path(&self.cwd, path))
                    .then_some(index)
            })
            .collect())
    }
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

#[derive(Debug, Clone)]
pub(super) struct FileData {
    pub(super) path: PathBuf,
    pub(super) relative: String,
    pub(super) exports: BTreeMap<String, ExportInfo>,
    pub(super) imports: Vec<ImportRecord>,
    pub(super) side_effect_imports: Vec<String>,
    pub(super) dynamic_imports: Vec<String>,
    pub(super) glob_imports: Vec<String>,
    pub(super) namespace_imports: BTreeMap<String, String>,
    pub(super) named_imports: BTreeMap<String, NamedImport>,
    pub(super) member_uses: Vec<(String, String)>,
    pub(super) reexport_all: Vec<String>,
    pub(super) local_references: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ExportInfo {
    pub(super) kind: ExportKind,
    pub(super) local_name: Option<String>,
    pub(super) name_span: Span,
    pub(super) line: usize,
    pub(super) col: usize,
    pub(super) remove_span: Span,
    pub(super) reexport_source: Option<String>,
    pub(super) reexport_name: Option<String>,
    pub(super) namespace_source: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ImportRecord {
    pub(super) source: String,
    pub(super) imported: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct NamedImport {
    pub(super) source: String,
    pub(super) imported: String,
}

struct FileVisitor {
    path: PathBuf,
    relative: String,
    exports: BTreeMap<String, ExportInfo>,
    imports: Vec<ImportRecord>,
    side_effect_imports: Vec<String>,
    dynamic_imports: Vec<String>,
    glob_imports: Vec<String>,
    namespace_imports: BTreeMap<String, String>,
    named_imports: BTreeMap<String, NamedImport>,
    member_uses: Vec<(String, String)>,
    reexport_all: Vec<String>,
    local_references: BTreeSet<String>,
}

impl FileVisitor {
    pub(super) fn new(cwd: &Path, path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            relative: relative_path(cwd, path),
            exports: BTreeMap::new(),
            imports: Vec::new(),
            side_effect_imports: Vec::new(),
            dynamic_imports: Vec::new(),
            glob_imports: Vec::new(),
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
            dynamic_imports: self.dynamic_imports,
            glob_imports: self.glob_imports,
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

    fn add_dynamic_import_binding(&mut self, pattern: &BindingPattern<'_>, source: &str) {
        if let Some(local) = binding_identifier_name(pattern) {
            self.side_effect_imports.push(source.to_string());
            self.namespace_imports.insert(local, source.to_string());
            return;
        }

        let mut names = Vec::new();
        collect_imported_binding_names(pattern, &mut names);
        for name in names {
            self.imports.push(ImportRecord {
                source: source.to_string(),
                imported: Some(name),
            });
        }
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
                self.add_dynamic_import_binding(&declaration.id, &source);
            }
        }
        walk::walk_variable_declarator(self, declaration);
    }

    fn visit_call_expression(&mut self, expression: &CallExpression<'a>) {
        self.glob_imports
            .extend(import_meta_glob_patterns(expression));

        walk::walk_call_expression(self, expression);
    }

    fn visit_import_expression(&mut self, expression: &ImportExpression<'a>) {
        if let Expression::StringLiteral(source) = &expression.source {
            self.dynamic_imports.push(source.value.as_str().to_string());
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

fn collect_imported_binding_names(pattern: &BindingPattern<'_>, names: &mut Vec<String>) {
    match pattern {
        BindingPattern::ObjectPattern(pattern) => {
            for property in &pattern.properties {
                if !property.computed {
                    if let Some(name) = property_key_name(&property.key) {
                        names.push(name);
                        continue;
                    }
                }
                collect_binding_names(&property.value, names);
            }
        }
        BindingPattern::AssignmentPattern(pattern) => {
            collect_imported_binding_names(&pattern.left, names);
        }
        _ => collect_binding_names(pattern, names),
    }
}

fn binding_identifier_name(pattern: &BindingPattern<'_>) -> Option<String> {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => Some(identifier.name.as_str().to_string()),
        BindingPattern::AssignmentPattern(pattern) => binding_identifier_name(&pattern.left),
        _ => None,
    }
}

fn property_key_name(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.as_str().to_string()),
        PropertyKey::StringLiteral(literal) => Some(literal.value.as_str().to_string()),
        _ => None,
    }
}

fn import_source_from_expression(expression: &Expression<'_>) -> Option<String> {
    match expression {
        Expression::ImportExpression(import) => match &import.source {
            Expression::StringLiteral(source) => Some(source.value.as_str().to_string()),
            _ => None,
        },
        Expression::CallExpression(call) if is_require_call(call) => {
            call.arguments.first().and_then(argument_string_literal)
        }
        Expression::AwaitExpression(await_expression) => {
            import_source_from_expression(&await_expression.argument)
        }
        Expression::ParenthesizedExpression(parenthesized) => {
            import_source_from_expression(&parenthesized.expression)
        }
        _ => None,
    }
}

fn is_require_call(call: &CallExpression<'_>) -> bool {
    matches!(&call.callee, Expression::Identifier(identifier) if identifier.name == "require")
}

fn argument_string_literal(argument: &Argument<'_>) -> Option<String> {
    match argument {
        Argument::StringLiteral(source) => Some(source.value.as_str().to_string()),
        _ => None,
    }
}

fn import_meta_glob_patterns(call: &CallExpression<'_>) -> Vec<String> {
    if !is_import_meta_glob_callee(&call.callee) {
        return Vec::new();
    }

    call.arguments
        .first()
        .map(import_meta_glob_argument_patterns)
        .unwrap_or_default()
}

fn import_meta_glob_argument_patterns(argument: &Argument<'_>) -> Vec<String> {
    match argument {
        Argument::StringLiteral(source) => vec![source.value.as_str().to_string()],
        Argument::ArrayExpression(array) => array
            .elements
            .iter()
            .filter_map(|element| match element {
                ArrayExpressionElement::StringLiteral(source) => {
                    Some(source.value.as_str().to_string())
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn is_import_meta_glob_callee(callee: &Expression<'_>) -> bool {
    match callee {
        Expression::StaticMemberExpression(member) if member.property.name == "glob" => {
            matches!(
                &member.object,
                Expression::MetaProperty(meta)
                    if meta.meta.name == "import" && meta.property.name == "meta"
            )
        }
        _ => false,
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

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
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

fn line_col_from_starts(source: &str, line_starts: &[usize], offset: u32) -> (usize, usize) {
    let offset = offset as usize;
    let line_index = line_starts
        .partition_point(|line_start| *line_start <= offset)
        .saturating_sub(1);
    let line_start = line_starts[line_index];
    let col = source[line_start..offset].chars().count() + 1;
    (line_index + 1, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_start_lookup_matches_scanning_line_col() {
        let source = "αβ\nconst value = 1;\nexport { value };\n";
        let line_starts = line_starts(source);
        for offset in source
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(source.len()))
        {
            assert_eq!(
                line_col(source, offset as u32),
                line_col_from_starts(source, &line_starts, offset as u32)
            );
        }
    }
}
