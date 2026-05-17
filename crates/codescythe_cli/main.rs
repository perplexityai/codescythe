use std::{path::PathBuf, process::ExitCode};

use anyhow::Result;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about = "Find focused TypeScript dead code")]
struct Args {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(short = 'C', long, default_value = ".")]
    directory: PathBuf,

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
    let cwd = args.directory.canonicalize()?;
    let config = args.config.as_deref();

    if args.fix {
        let result = codescythe::run_and_fix(&cwd, config)?;
        if args.json {
            println!("{}", serde_json::to_string_pretty(&result)?);
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
        println!("{}", serde_json::to_string_pretty(&analysis)?);
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
