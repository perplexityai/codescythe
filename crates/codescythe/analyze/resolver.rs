use super::*;

pub(super) struct ModuleResolver {
    resolver: ResolverGeneric<IgnoredResolverMetadataFileSystem>,
    index_by_path: HashMap<PathBuf, usize>,
}

struct IgnoredResolverMetadataFileSystem {
    cwd: PathBuf,
    ignore: GlobSet,
    inner: FileSystemOs,
}

impl IgnoredResolverMetadataFileSystem {
    fn for_config(cwd: &Path, config: &CodescytheConfig) -> Result<Self> {
        Ok(Self {
            cwd: cwd.to_path_buf(),
            ignore: build_glob_set(&config.ignore)?,
            inner: FileSystemOs::new(),
        })
    }

    fn ignores_resolver_metadata(&self, path: &Path) -> bool {
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, "package.json" | "tsconfig.json"))
        {
            return false;
        }

        if path
            .components()
            .any(|component| component.as_os_str() == "node_modules")
        {
            return false;
        }

        let normalized = normalize_path(path);
        self.ignore.is_match(relative_path(&self.cwd, &normalized))
    }

    fn ignored_error(path: &Path) -> io::Error {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("ignored by codescythe config: {}", path.display()),
        )
    }
}

impl FileSystem for IgnoredResolverMetadataFileSystem {
    fn new() -> Self {
        Self {
            cwd: PathBuf::new(),
            ignore: build_glob_set(&[]).expect("empty glob set is valid"),
            inner: FileSystemOs::new(),
        }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if self.ignores_resolver_metadata(path) {
            return Err(Self::ignored_error(path));
        }
        self.inner.read(path)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        if self.ignores_resolver_metadata(path) {
            return Err(Self::ignored_error(path));
        }
        self.inner.read_to_string(path)
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        if self.ignores_resolver_metadata(path) {
            return Err(Self::ignored_error(path));
        }
        self.inner.metadata(path)
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        if self.ignores_resolver_metadata(path) {
            return Err(Self::ignored_error(path));
        }
        self.inner.symlink_metadata(path)
    }

    fn read_link(&self, path: &Path) -> Result<PathBuf, ResolveError> {
        self.inner.read_link(path)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.inner.canonicalize(path)
    }
}

pub(super) enum ImportResolution {
    Project(usize),
    External,
    Unresolved,
}

impl ModuleResolver {
    pub(super) fn new(
        cwd: &Path,
        project_files: &[PathBuf],
        config: &CodescytheConfig,
    ) -> Result<Self> {
        let resolver = ResolverGeneric::new_with_file_system(
            IgnoredResolverMetadataFileSystem::for_config(cwd, config)?,
            ResolveOptions {
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
            },
        );
        let index_by_path = project_files
            .iter()
            .enumerate()
            .map(|(index, path)| (normalize_path(path), index))
            .collect::<HashMap<_, _>>();

        Ok(Self {
            resolver,
            index_by_path,
        })
    }

    pub(super) fn resolve(&self, from: &FileData, specifier: &str) -> Result<ImportResolution> {
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

    pub(super) fn explain_unresolved(
        &self,
        cwd: &Path,
        config: &CodescytheConfig,
        importer: &str,
        specifier: &str,
    ) -> Result<UnresolvedImportExplanation> {
        let importer_path = cwd.join(importer);
        let resolver_error = match self.resolver.resolve_file(&importer_path, specifier) {
            Ok(resolution) => format!(
                "resolved to {} but did not map to a project file",
                relative_path(cwd, resolution.path())
            ),
            Err(error) => error.to_string(),
        };
        let aliases = source_alias_mappings(cwd, config)?;
        let matched_aliases = aliases
            .iter()
            .flat_map(|alias| self.matched_alias_explanations(cwd, alias, specifier))
            .collect();

        Ok(UnresolvedImportExplanation {
            importer: importer.to_string(),
            specifier: specifier.to_string(),
            resolver_error,
            matched_aliases,
        })
    }

    fn matched_alias_explanations(
        &self,
        cwd: &Path,
        alias: &AliasMapping,
        specifier: &str,
    ) -> Vec<UnresolvedImportMatchedAlias> {
        let Some(tail) = alias_match_tail(&alias.key, specifier) else {
            return Vec::new();
        };

        alias
            .values
            .iter()
            .map(|target| {
                let expanded_target = expand_alias_target(target, &tail);
                let candidate_files = candidate_paths_for_expanded_target(&expanded_target)
                    .into_iter()
                    .map(|candidate| self.candidate_file(cwd, &candidate))
                    .collect();
                UnresolvedImportMatchedAlias {
                    source: alias.source.clone(),
                    key: alias.key.clone(),
                    target: target.clone(),
                    expanded_target,
                    candidate_files,
                }
            })
            .collect()
    }

    fn candidate_file(&self, cwd: &Path, candidate: &str) -> UnresolvedImportCandidateFile {
        let path = candidate_path(cwd, candidate);
        let normalized = normalize_path(&path);
        UnresolvedImportCandidateFile {
            path: display_candidate_path(cwd, &normalized),
            exists: normalized.exists(),
            in_project: self.index_by_path.contains_key(&normalized),
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

pub(super) fn source_alias_mappings(
    cwd: &Path,
    config: &CodescytheConfig,
) -> Result<Vec<AliasMapping>> {
    let mut aliases = package_import_aliases(cwd, config)?;
    aliases.extend(config.aliases.iter().map(|(key, values)| AliasMapping {
        key: key.clone(),
        values: values.clone(),
        source: "config.aliases".to_string(),
    }));
    Ok(aliases)
}

fn package_import_aliases(cwd: &Path, config: &CodescytheConfig) -> Result<Vec<AliasMapping>> {
    let package_json = cwd.join("package.json");
    if !package_json.exists() || config_ignores_path(cwd, config, &package_json)? {
        return Ok(Vec::new());
    }
    let value = serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&package_json)?)
        .with_context(|| format!("failed to parse {}", package_json.display()))?;
    let Some(imports) = value.get("imports").and_then(|value| value.as_object()) else {
        return Ok(Vec::new());
    };

    Ok(imports
        .iter()
        .map(|(key, value)| AliasMapping {
            key: key.clone(),
            values: collect_import_targets(value),
            source: "package.json#imports".to_string(),
        })
        .collect())
}

pub(super) fn package_import_keys(cwd: &Path, config: &CodescytheConfig) -> Result<Vec<String>> {
    let mut keys = package_import_aliases(cwd, config)?
        .into_iter()
        .map(|alias| alias.key)
        .collect::<Vec<_>>();
    keys.sort();
    Ok(keys)
}

fn config_ignores_path(cwd: &Path, config: &CodescytheConfig, path: &Path) -> Result<bool> {
    Ok(build_glob_set(&config.ignore)?.is_match(relative_path(cwd, &normalize_path(path))))
}

fn collect_import_targets(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(value) => vec![value.clone()],
        serde_json::Value::Array(values) => {
            values.iter().flat_map(collect_import_targets).collect()
        }
        serde_json::Value::Object(values) => {
            values.values().flat_map(collect_import_targets).collect()
        }
        _ => Vec::new(),
    }
}

pub fn source_alias_ignore_warnings_for_config(
    cwd: &Path,
    config: &CodescytheConfig,
) -> Result<Vec<SourceAliasIgnoreWarning>> {
    let aliases = source_alias_mappings(cwd, config)?;
    source_alias_ignore_warnings(config, &aliases)
}

pub fn source_alias_fix_blocking_ignore_warnings_for_config(
    cwd: &Path,
    config: &CodescytheConfig,
) -> Result<Vec<SourceAliasIgnoreWarning>> {
    Ok(source_alias_ignore_warnings_for_config(cwd, config)?
        .into_iter()
        .filter(|warning| warning.fix_blocking)
        .collect())
}

pub(super) fn source_alias_ignore_warnings(
    config: &CodescytheConfig,
    aliases: &[AliasMapping],
) -> Result<Vec<SourceAliasIgnoreWarning>> {
    let mut warnings = Vec::new();
    for pattern in &config.unresolved_imports.ignore {
        let literal_prefix = glob_literal_prefix(pattern);
        let fix_blocking = unresolved_ignore_can_match_source_module(pattern);
        for alias in aliases {
            let Some(alias_prefix) = alias_literal_prefix(&alias.key) else {
                continue;
            };
            if literal_prefix.starts_with(&alias_prefix)
                || alias_prefix.starts_with(&literal_prefix)
            {
                let message = if fix_blocking {
                    format!(
                        "unresolved import ignore pattern {pattern:?} overlaps local source alias {:?} and can hide JS/TS source imports",
                        alias.key
                    )
                } else {
                    format!(
                        "unresolved import ignore pattern {pattern:?} overlaps local source alias {:?} but only matches non-JS/TS asset-like imports",
                        alias.key
                    )
                };
                warnings.push(SourceAliasIgnoreWarning {
                    pattern: pattern.clone(),
                    alias: alias.key.clone(),
                    source: alias.source.clone(),
                    fix_blocking,
                    message,
                });
            }
        }
    }
    warnings.sort_by(|left, right| {
        left.pattern
            .cmp(&right.pattern)
            .then(left.alias.cmp(&right.alias))
            .then(left.source.cmp(&right.source))
    });
    warnings.dedup_by(|left, right| {
        left.pattern == right.pattern && left.alias == right.alias && left.source == right.source
    });
    Ok(warnings)
}

fn alias_literal_prefix(alias: &str) -> Option<String> {
    let prefix = glob_literal_prefix(alias);
    (!prefix.is_empty()).then_some(prefix)
}

fn glob_literal_prefix(pattern: &str) -> String {
    let mut prefix = String::new();
    for character in pattern.chars() {
        if matches!(character, '*' | '?' | '[' | '{') {
            break;
        }
        prefix.push(character);
    }
    prefix
}

fn unresolved_ignore_can_match_source_module(pattern: &str) -> bool {
    let without_query = pattern
        .split_once('?')
        .map_or(pattern, |(prefix, _)| prefix);
    let segment = without_query.rsplit('/').next().unwrap_or(without_query);
    let Some((_, extension)) = segment.rsplit_once('.') else {
        return true;
    };
    if extension.is_empty()
        || extension
            .chars()
            .any(|character| matches!(character, '*' | '?' | '[' | '{'))
    {
        return true;
    }
    matches!(
        extension,
        "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs"
    )
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

fn alias_match_tail(alias: &str, specifier: &str) -> Option<String> {
    let Some(wildcard) = alias.find('*') else {
        return (alias == specifier).then(String::new);
    };

    let prefix = &alias[..wildcard];
    let suffix = &alias[wildcard + 1..];
    specifier
        .strip_prefix(prefix)?
        .strip_suffix(suffix)
        .map(ToString::to_string)
}

fn expand_alias_target(target: &str, tail: &str) -> String {
    if let Some(wildcard) = target.find('*') {
        format!("{}{}{}", &target[..wildcard], tail, &target[wildcard + 1..])
    } else {
        target.to_string()
    }
}

fn candidate_paths_for_expanded_target(expanded_target: &str) -> Vec<String> {
    let without_query = expanded_target
        .split_once('?')
        .map_or(expanded_target, |(prefix, _)| prefix);
    let normalized = without_query.replace('\\', "/");

    for (extension, aliases) in [
        (".js", [".ts", ".tsx", ".js", ".jsx"].as_slice()),
        (".jsx", &[".tsx", ".jsx"]),
        (".mjs", &[".mts", ".mjs"]),
        (".cjs", &[".cts", ".cjs"]),
    ] {
        if let Some(base) = normalized.strip_suffix(extension) {
            return aliases
                .iter()
                .map(|alias| format!("{base}{alias}"))
                .collect();
        }
    }

    if path_has_extension(&normalized) {
        return vec![normalized];
    }

    [
        ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs", ".json", ".node",
    ]
    .into_iter()
    .map(|extension| format!("{normalized}{extension}"))
    .collect()
}

fn path_has_extension(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .and_then(|segment| segment.rsplit_once('.'))
        .is_some_and(|(_, extension)| !extension.is_empty())
}

fn candidate_path(cwd: &Path, candidate: &str) -> PathBuf {
    let path = Path::new(candidate);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    cwd.join(candidate)
}

fn display_candidate_path(cwd: &Path, path: &Path) -> String {
    path.strip_prefix(cwd)
        .map_or_else(|_| path, |relative| relative)
        .to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

pub(super) struct UnresolvedImportPolicy {
    mode: UnresolvedImportsMode,
    ignore: Vec<(String, GlobMatcher)>,
}

impl UnresolvedImportPolicy {
    pub(super) fn new(config: &CodescytheConfig) -> Result<Self> {
        let mut ignore = Vec::new();
        for pattern in &config.unresolved_imports.ignore {
            ignore.push((
                pattern.clone(),
                Glob::new(pattern)
                    .with_context(|| format!("invalid glob pattern {pattern:?}"))?
                    .compile_matcher(),
            ));
        }
        Ok(Self {
            mode: config.unresolved_imports.mode,
            ignore,
        })
    }

    pub(super) fn record(
        &self,
        unresolved: &mut UnresolvedImports,
        ignored: &mut BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
        importer: &str,
        specifier: &str,
    ) -> Result<()> {
        if let Some(pattern) = self.ignored_pattern(specifier) {
            let entry = ignored.entry(pattern.to_string()).or_insert_with(|| {
                IgnoredUnresolvedImportsByPattern {
                    pattern: pattern.to_string(),
                    count: 0,
                    samples: Vec::new(),
                }
            });
            entry.count += 1;
            if entry.samples.len() < IGNORED_UNRESOLVED_SAMPLE_LIMIT {
                entry.samples.push(IgnoredUnresolvedImportSample {
                    specifier: specifier.to_string(),
                    importer: importer.to_string(),
                });
            }
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

    fn ignored_pattern(&self, specifier: &str) -> Option<&str> {
        self.ignore
            .iter()
            .find_map(|(pattern, matcher)| matcher.is_match(specifier).then_some(pattern.as_str()))
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
