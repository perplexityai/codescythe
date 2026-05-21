use std::{
    env,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about = "Find focused TypeScript dead code")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(short = 'C', long)]
    directory: Option<PathBuf>,

    #[arg(long)]
    fix: bool,

    #[arg(long)]
    force: bool,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    verbose: bool,

    #[arg(long)]
    explain_export: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Doctor(DoctorArgs),
}

#[derive(Debug, Parser)]
struct DoctorArgs {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(short = 'C', long)]
    directory: Option<PathBuf>,

    #[arg(long)]
    json: bool,
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
    if let Some(command) = args.command {
        return run_command(command);
    }

    let config = args.config.as_deref();
    let cwd = analysis_root(args.directory.as_deref(), config)?;

    if args.fix {
        let result = codescythe::run_and_fix_with_options(
            &cwd,
            config,
            codescythe::FixOptions {
                verbose: args.verbose,
                force: args.force,
            },
        )?;
        if args.json {
            println!("{}", serde_json::to_string(&result)?);
        } else {
            println!(
                "Removed {} unused exports from {} files and {} unused files",
                result.removed_exports,
                result.changed_files.len(),
                result.removed_files.len()
            );
            if !result.skipped_export_files.is_empty() {
                println!(
                    "Skipped export edits in {} files because ignored unresolved imports create alias uncertainty",
                    result.skipped_export_files.len()
                );
                for file in &result.skipped_export_files {
                    println!("  {file}");
                }
            }
        }
        return Ok(!result.analysis.issues.files.is_empty()
            || !result.analysis.issues.exports.is_empty()
            || !result.analysis.issues.unresolved.is_empty());
    }

    let explain_export = args
        .explain_export
        .as_deref()
        .map(parse_explain_export)
        .transpose()?;
    let analysis = codescythe::run_with_options(
        &cwd,
        config,
        codescythe::AnalysisOptions {
            verbose: args.verbose || explain_export.is_some(),
            explain_export,
            ..codescythe::AnalysisOptions::default()
        },
    )?;
    if args.explain_export.is_some() && !args.json {
        print_explain_export(&analysis);
        return Ok(!matches!(
            analysis.explain_export.as_ref().map(|result| result.status),
            Some(codescythe::ExplainExportStatus::Alive)
        ));
    }
    if args.json {
        println!("{}", serde_json::to_string(&analysis)?);
    } else {
        print_text_report(&analysis);
    }

    Ok(!analysis.issues.files.is_empty()
        || !analysis.issues.exports.is_empty()
        || !analysis.issues.unresolved.is_empty())
}

fn run_command(command: Command) -> Result<bool> {
    match command {
        Command::Doctor(args) => {
            let config = args.config.as_deref();
            let cwd = analysis_root(args.directory.as_deref(), config)?;
            let result = codescythe::doctor(&cwd, config)?;
            if args.json {
                println!("{}", serde_json::to_string(&result)?);
            } else {
                print_doctor_report(&result);
            }
            Ok(!result.warnings.is_empty())
        }
    }
}

fn print_text_report(analysis: &codescythe::Analysis) {
    if !analysis.source_alias_ignore_warnings.is_empty() {
        println!("Warnings ({})", analysis.source_alias_ignore_warnings.len());
        for warning in &analysis.source_alias_ignore_warnings {
            println!("  {}", warning.message);
        }
    }

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

    if !analysis.ignored_unresolved_imports_by_pattern.is_empty() {
        println!(
            "Ignored unresolved imports ({})",
            analysis.counters.ignored_unresolved
        );
        for ignored in analysis.ignored_unresolved_imports_by_pattern.values() {
            println!("  {} ignored {} imports", ignored.pattern, ignored.count);
            for sample in &ignored.samples {
                println!("    {} from {}", sample.specifier, sample.importer);
            }
        }
    }
}

fn parse_explain_export(value: &str) -> Result<codescythe::ExplainExportRequest> {
    let Some((file, symbol)) = value.rsplit_once(':') else {
        anyhow::bail!("--explain-export must be formatted as <file>:<symbol>");
    };
    if file.is_empty() || symbol.is_empty() {
        anyhow::bail!("--explain-export must include both file and symbol");
    }
    Ok(codescythe::ExplainExportRequest {
        file: file.to_string(),
        symbol: symbol.to_string(),
    })
}

fn print_explain_export(analysis: &codescythe::Analysis) {
    let Some(result) = &analysis.explain_export else {
        return;
    };
    println!(
        "{}:{} is {:?}: {}",
        result.exporting_file, result.symbol, result.status, result.reason
    );
    if let Some(explanation) = &result.explanation {
        println!("  file reachable: {}", explanation.file_reachable);
        if !explanation.importers_considered.is_empty() {
            println!("  importers considered:");
            for importer in &explanation.importers_considered {
                println!(
                    "    {} imports {} ({})",
                    importer.importer, importer.specifier, importer.reason
                );
            }
        }
        if !explanation.importers_skipped.is_empty() {
            println!("  importers skipped:");
            for importer in &explanation.importers_skipped {
                println!(
                    "    {} imports {} ({})",
                    importer.importer, importer.specifier, importer.reason
                );
            }
        }
        if !explanation
            .ignored_unresolved_imports_that_might_have_pointed_at_this_file
            .is_empty()
        {
            println!("  ignored unresolved imports that might point here:");
            for ignored in
                &explanation.ignored_unresolved_imports_that_might_have_pointed_at_this_file
            {
                println!("    {} from {}", ignored.specifier, ignored.importer);
            }
        }
    }
}

fn print_doctor_report(result: &codescythe::ConfigDoctorResult) {
    if result.warnings.is_empty() {
        println!("No risky Codescythe config found");
        return;
    }

    println!("Config warnings ({})", result.warnings.len());
    for warning in &result.warnings {
        println!("  {}: {}", warning.code, warning.message);
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
