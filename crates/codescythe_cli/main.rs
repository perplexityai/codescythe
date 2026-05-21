use std::{
    env,
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

use anyhow::Result;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about = "Find focused TypeScript dead code")]
struct Args {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(short = 'C', long)]
    directory: Option<PathBuf>,

    #[arg(long)]
    fix: bool,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    verbose: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(has_issues) => {
            if has_issues {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let args = Args::parse();
    let config = args.config.as_deref();
    let cwd = analysis_root(args.directory.as_deref(), config)?;
    let options = codescythe::AnalysisOptions {
        diagnostics: args.verbose,
        ..Default::default()
    };

    if args.fix {
        let loaded_config = codescythe::load_config_with_source(&cwd, config)?;
        let analysis = codescythe::analyze_path(&cwd, &loaded_config.config, options)?;
        let mut result = codescythe::apply_fixes(&cwd, &analysis)?;
        if args.verbose {
            attach_runtime_diagnostics(&mut result.analysis, &args, &cwd, &loaded_config.source)?;
            let fix_plan = codescythe::fix_plan_diagnostics(&analysis, &result);
            if let Some(diagnostics) = result.analysis.diagnostics.as_mut() {
                diagnostics.fix_plan = Some(fix_plan);
            }
        }
        if args.json {
            println!("{}", serde_json::to_string(&result)?);
        } else {
            if args.verbose {
                print_diagnostics(&result.analysis);
            }
            if has_issues(&result.analysis) {
                print_text_report(&result.analysis);
                println!();
            }
            println!(
                "Removed {} unused exports from {} files and {} unused files",
                result.removed_exports,
                result.changed_files.len(),
                result.removed_files.len()
            );
        }
        return Ok(has_issues(&result.analysis));
    }

    let loaded_config = codescythe::load_config_with_source(&cwd, config)?;
    let mut analysis = codescythe::analyze_path(&cwd, &loaded_config.config, options)?;
    if args.verbose {
        attach_runtime_diagnostics(&mut analysis, &args, &cwd, &loaded_config.source)?;
    }
    if args.json {
        println!("{}", serde_json::to_string(&analysis)?);
    } else {
        if args.verbose {
            print_diagnostics(&analysis);
        }
        print_text_report(&analysis);
    }

    Ok(has_issues(&analysis))
}

fn attach_runtime_diagnostics(
    analysis: &mut codescythe::Analysis,
    args: &Args,
    cwd: &Path,
    config_source: &codescythe::ConfigSource,
) -> Result<()> {
    let Some(diagnostics) = analysis.diagnostics.as_mut() else {
        return Ok(());
    };
    diagnostics.runtime = Some(codescythe::RuntimeDiagnostics {
        version: format!("codescythe {}", env!("CARGO_PKG_VERSION")),
        process_cwd: env::current_dir()?.to_string_lossy().replace('\\', "/"),
        resolved_directory: display_path(cwd)?,
        config_source: codescythe::RuntimeConfigSource {
            kind: config_source_kind(config_source.kind).to_string(),
            path: config_source
                .path
                .as_ref()
                .map(|path| display_path(path))
                .transpose()?,
        },
        fix: args.fix,
        json: args.json,
        verbose: args.verbose,
    });
    Ok(())
}

fn config_source_kind(kind: codescythe::ConfigSourceKind) -> &'static str {
    match kind {
        codescythe::ConfigSourceKind::Cli => "cli",
        codescythe::ConfigSourceKind::Discovered => "discovered",
        codescythe::ConfigSourceKind::PackageJson => "packageJson",
        codescythe::ConfigSourceKind::Default => "default",
    }
}

fn print_diagnostics(analysis: &codescythe::Analysis) {
    let Some(diagnostics) = &analysis.diagnostics else {
        return;
    };
    eprintln!("Codescythe diagnostics");
    if let Some(runtime) = &diagnostics.runtime {
        eprintln!("Runtime:");
        eprintln!("  version: {}", runtime.version);
        eprintln!("  process cwd: {}", runtime.process_cwd);
        eprintln!("  resolved directory: {}", runtime.resolved_directory);
        match &runtime.config_source.path {
            Some(path) => eprintln!("  config: {} ({})", path, runtime.config_source.kind),
            None => eprintln!("  config: <default> ({})", runtime.config_source.kind),
        }
        eprintln!(
            "  flags: fix={} json={} verbose={}",
            runtime.fix, runtime.json, runtime.verbose
        );
    }

    eprintln!("Config:");
    eprintln!("  entry: {}", diagnostics.config.entry.join(", "));
    eprintln!("  project: {}", diagnostics.config.project.join(", "));
    eprintln!("  ignore: {}", diagnostics.config.ignore.join(", "));
    eprintln!(
        "  testFilePatterns: {}",
        diagnostics.config.test_file_patterns.join(", ")
    );
    eprintln!(
        "  unresolvedImports: mode={:?} ignore={}",
        diagnostics.config.unresolved_imports.mode,
        diagnostics.config.unresolved_imports.ignore.join(", ")
    );
    let package_imports = &diagnostics.config.aliases.package_json_imports;
    if !diagnostics.config.aliases.configured.is_empty() {
        eprintln!(
            "  configured aliases: {}",
            diagnostics
                .config
                .aliases
                .configured
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if let Some(path) = &package_imports.path {
        eprintln!(
            "  package.json#imports: {} ({})",
            path,
            package_imports.keys.join(", ")
        );
    }

    let discovery = &diagnostics.file_discovery;
    eprintln!("File discovery:");
    eprintln!("  project matched: {}", discovery.project_matched);
    eprintln!(
        "  selected project files: {}",
        discovery.selected_project_files
    );
    eprintln!(
        "  ignored by .gitignore: {}",
        discovery.ignored_by_gitignore
    );
    eprintln!("  ignored by config: {}", discovery.ignored_by_config);
    eprintln!("  parsed: {}", discovery.parsed);
    eprintln!(
        "  skipped by extension/type: {}",
        discovery.skipped_by_extension_or_type
    );
    eprintln!("  entries: {}", discovery.entries);
    eprintln!("  test leaf files: {}", discovery.test_leaf_files);

    eprintln!("Entry matches:");
    for (pattern, matches) in &diagnostics.entry.entry_matches_by_pattern {
        eprintln!("  {pattern}: {}", matches.len());
    }
    if !diagnostics.entry.zero_match_patterns.is_empty() {
        eprintln!(
            "  zero-match entry patterns: {}",
            diagnostics.entry.zero_match_patterns.join(", ")
        );
    }

    if !diagnostics.dead_files.is_empty() {
        eprintln!("Dead file reasons:");
        for (path, reason) in &diagnostics.dead_files {
            eprintln!("  {path}: {}", reason.reason);
            eprintln!(
                "    entry={} test={} imported={} onlyDeadOrTestImporters={} testLeafSkipped={}",
                reason.matched_entry,
                reason.matched_test_file_patterns,
                reason.imported,
                reason.only_imported_by_dead_or_test_files,
                reason.skipped_from_reachability_due_to_test_leaf_semantics
            );
            if !reason.imported_by.is_empty() {
                eprintln!("    imported by: {}", reason.imported_by.join(", "));
            }
        }
    }

    if let Some(fix_plan) = &diagnostics.fix_plan {
        eprintln!("Fix plan:");
        eprintln!("  delete files: {}", fix_plan.files_to_delete.join(", "));
        for (path, symbols) in &fix_plan.files_with_export_edits {
            eprintln!("  edit exports in {path}: {}", symbols.join(", "));
        }
        for (path, symbols) in &fix_plan.skipped_exports_in_deleted_files {
            eprintln!(
                "  skip exports in deleted file {path}: {}",
                symbols.join(", ")
            );
        }
    }
}

fn has_issues(analysis: &codescythe::Analysis) -> bool {
    !analysis.issues.files.is_empty()
        || !analysis.issues.exports.is_empty()
        || !analysis.issues.unresolved.is_empty()
}

fn print_text_report(analysis: &codescythe::Analysis) {
    if analysis.issues.files.is_empty()
        && analysis.issues.exports.is_empty()
        && analysis.issues.unresolved.is_empty()
    {
        println!("No dead TypeScript code found");
        return;
    }

    if !analysis.issues.files.is_empty() {
        println!("Unused files ({})", analysis.issues.files.len());
        for path in analysis.issues.files.keys() {
            println!("  {path}");
        }
    }

    let export_count = analysis
        .issues
        .exports
        .values()
        .map(std::collections::BTreeMap::len)
        .sum::<usize>();
    if export_count > 0 {
        println!("Unused exports ({export_count})");
        for (path, exports) in &analysis.issues.exports {
            for issue in exports.values() {
                println!("  {}:{}:{} {}", path, issue.line, issue.col, issue.symbol);
            }
        }
    }

    if !analysis.issues.unresolved.is_empty() {
        println!("Unresolved imports ({})", analysis.counters.unresolved);
        for (path, imports) in &analysis.issues.unresolved {
            for import in imports {
                println!("  {path}: {import}");
            }
        }
    }
}

fn analysis_root(directory: Option<&Path>, config: Option<&Path>) -> Result<PathBuf> {
    if let Some(directory) = directory {
        return Ok(directory.to_path_buf());
    }

    if let Some(parent) = config
        .and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        return Ok(parent.to_path_buf());
    }

    Ok(env::current_dir()?)
}

fn display_path(path: &Path) -> Result<String> {
    Ok(absolute_normalize_path(path)?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn absolute_normalize_path(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    Ok(normalize_path(&path))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_analysis_root_from_config_parent_without_directory() {
        let root = Path::new("/tmp/runfiles/_main");
        let config = root.join("codescythe.json");

        let analysis_root = analysis_root(None, Some(&config)).unwrap();

        assert_eq!(analysis_root, root);
    }

    #[test]
    fn explicit_directory_overrides_config_parent() {
        let directory = Path::new("/tmp/runfiles/_main");
        let config = Path::new("/tmp/runfiles/_main/workspace/frontend/codescythe.json");

        let analysis_root = analysis_root(Some(directory), Some(config)).unwrap();

        assert_eq!(analysis_root, directory);
    }
}
