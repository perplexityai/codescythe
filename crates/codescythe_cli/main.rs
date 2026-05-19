use std::{
    env,
    path::{Path, PathBuf},
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

    if args.fix {
        let result = codescythe::run_and_fix(&cwd, config)?;
        if args.json {
            println!("{}", serde_json::to_string(&result)?);
        } else {
            println!(
                "Removed {} unused exports from {} files",
                result.removed_exports,
                result.changed_files.len()
            );
        }
        return Ok(!result.analysis.issues.files.is_empty()
            || !result.analysis.issues.exports.is_empty()
            || !result.analysis.issues.unresolved.is_empty());
    }

    let analysis = codescythe::run(&cwd, config)?;
    if args.json {
        println!("{}", serde_json::to_string(&analysis)?);
    } else {
        print_text_report(&analysis);
    }

    Ok(!analysis.issues.files.is_empty()
        || !analysis.issues.exports.is_empty()
        || !analysis.issues.unresolved.is_empty())
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
