use super::*;

const DOCTOR_UNRESOLVED_IMPORT_LIMIT: usize = 10;

pub fn doctor_config(
    cwd: &Path,
    config: &CodescytheConfig,
    config_path: Option<&Path>,
) -> Result<ConfigDoctorResult> {
    let cwd = absolute_normalize_path(cwd)?;
    let project_files = discover_project_files(&cwd, config)?;
    let entry_files = discover_entry_files(&cwd, config, &project_files)?;
    let analysis = if entry_files.is_empty() {
        None
    } else {
        Some(analyze_path(
            &cwd,
            config,
            AnalysisOptions {
                config_path: config_path.map(Path::to_path_buf),
                ..AnalysisOptions::default()
            },
        )?)
    };
    let mut warnings = Vec::new();

    for warning in source_alias_ignore_warnings_for_config(&cwd, config)? {
        warnings.push(ConfigDoctorWarning {
            code: "sourceAliasUnresolvedIgnore".to_string(),
            message: warning.message,
        });
    }

    for pattern in &config.entry {
        let matches = entry_pattern_matches(&cwd, pattern, &project_files)?;
        if matches == 0 {
            warnings.push(ConfigDoctorWarning {
                code: "entryGlobZeroMatches".to_string(),
                message: format!("entry pattern {pattern:?} matched no project files"),
            });
        }
    }

    if let Some(analysis) = &analysis
        && analysis.counters.unresolved > 0
    {
        warnings.push(ConfigDoctorWarning {
            code: "unresolvedImports".to_string(),
            message: format!(
                "analysis reported {} unresolved imports; inspect unresolvedImports for resolver diagnostics",
                analysis.counters.unresolved
            ),
        });
    }

    if project_files.len() >= 20
        && let Some(analysis) = &analysis
    {
        let unused_files = analysis.issues.files.len();
        if unused_files * 100 / project_files.len() >= 80 {
            warnings.push(ConfigDoctorWarning {
                code: "projectScopeMuchBroaderThanEntryCoverage".to_string(),
                message: format!(
                    "project has {} files but {} are currently reported unused from {} entries",
                    project_files.len(),
                    unused_files,
                    entry_files.len()
                ),
            });
        }
    }

    for (pattern, matches) in generated_ignore_matches_source_files(&cwd, config)? {
        warnings.push(ConfigDoctorWarning {
            code: "ignoredGeneratedPatternMatchesSource".to_string(),
            message: format!(
                "ignore pattern {pattern:?} contains generated but also matches checked source file {matches:?}"
            ),
        });
    }

    let unresolved_imports = analysis
        .as_ref()
        .map(|analysis| doctor_unresolved_imports(&cwd, config, &project_files, analysis))
        .transpose()?
        .unwrap_or_default();

    warnings.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then(left.message.cmp(&right.message))
    });
    warnings.dedup_by(|left, right| left.code == right.code && left.message == right.message);

    Ok(ConfigDoctorResult {
        warnings,
        summary: AnalysisSummary {
            version: env!("CARGO_PKG_VERSION").to_string(),
            config_path: config_path.map(|path| path.display().to_string()),
            project_count: project_files.len(),
            entry_count: entry_files.len(),
            ignored_unresolved_count: 0,
            ignored_unresolved_patterns: config.unresolved_imports.ignore.clone(),
            package_import_keys: package_import_keys(&cwd, config).unwrap_or_default(),
            configured_alias_keys: config.aliases.keys().cloned().collect(),
        },
        unresolved_imports,
    })
}

fn doctor_unresolved_imports(
    cwd: &Path,
    config: &CodescytheConfig,
    project_files: &[PathBuf],
    analysis: &Analysis,
) -> Result<Vec<UnresolvedImportExplanation>> {
    let resolver = ModuleResolver::new(cwd, project_files, config)?;
    let mut explanations = Vec::new();
    for (importer, specifiers) in &analysis.issues.unresolved {
        for specifier in specifiers {
            if explanations.len() >= DOCTOR_UNRESOLVED_IMPORT_LIMIT {
                return Ok(explanations);
            }
            explanations.push(resolver.explain_unresolved(cwd, config, importer, specifier)?);
        }
    }
    Ok(explanations)
}

fn entry_pattern_matches(cwd: &Path, pattern: &str, project_files: &[PathBuf]) -> Result<usize> {
    if !has_glob_meta(pattern) {
        let path = normalize_path(&cwd.join(pattern));
        return Ok(project_files
            .iter()
            .any(|project_file| normalize_path(project_file) == path) as usize);
    }
    let glob = build_glob_set(&[pattern.to_string()])?;
    Ok(project_files
        .iter()
        .filter(|path| glob.is_match(relative_path(cwd, path)))
        .count())
}

fn generated_ignore_matches_source_files(
    cwd: &Path,
    config: &CodescytheConfig,
) -> Result<Vec<(String, String)>> {
    let generated_patterns = config
        .ignore
        .iter()
        .filter(|pattern| pattern.contains("generated"))
        .cloned()
        .collect::<Vec<_>>();
    if generated_patterns.is_empty() {
        return Ok(Vec::new());
    }

    let include = build_glob_set(&config.project)?;
    let mut results = Vec::new();
    let matchers = generated_patterns
        .iter()
        .map(|pattern| {
            Glob::new(pattern)
                .with_context(|| format!("invalid glob pattern {pattern:?}"))
                .map(|glob| (pattern.clone(), glob.compile_matcher()))
        })
        .collect::<Result<Vec<_>>>()?;

    let empty_ignore = build_glob_set(&[])?;
    let filter_cwd = cwd.to_path_buf();
    let mut walker = WalkBuilder::new(cwd);
    walker
        .follow_links(true)
        .standard_filters(false)
        .git_ignore(true)
        .require_git(false);
    walker.filter_entry(move |entry| should_enter(&filter_cwd, entry, &empty_ignore));

    for entry in walker.build() {
        let entry = entry?;
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        let relative = relative_path(cwd, entry.path());
        if !include.is_match(&relative) {
            continue;
        }
        for (pattern, matcher) in &matchers {
            if matcher.is_match(&relative) {
                results.push((pattern.clone(), relative.clone()));
                break;
            }
        }
    }
    Ok(results)
}
