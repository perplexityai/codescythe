use std::{
    env,
    path::{Path, PathBuf},
    process::ExitCode,
};

#[cfg(feature = "profiling")]
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

#[cfg(feature = "profiling")]
const PROFILE_ENV: &str = "CODESCYTHE_PROFILE";

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
    Query(QueryArgs),
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

#[derive(Debug, Parser)]
struct QueryArgs {
    #[command(subcommand)]
    command: QueryCommand,
}

#[derive(Debug, Subcommand)]
enum QueryCommand {
    Somepath(QueryPathArgs),
    Somepaths(QueryPathArgs),
    Allpaths(QueryPathArgs),
}

#[derive(Debug, Parser)]
struct QueryPathArgs {
    from: String,
    to: String,

    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(short = 'C', long)]
    directory: Option<PathBuf>,

    #[arg(long)]
    json: bool,

    #[arg(long, value_enum, default_value_t = QueryOutputFormat::Text)]
    output: QueryOutputFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum QueryOutputFormat {
    Text,
    Json,
    Mermaid,
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
            let started = start_profile_timer();
            println!("{}", serde_json::to_string(&result)?);
            print_profile_stage("json serialization", started);
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
        let started = start_profile_timer();
        println!("{}", serde_json::to_string(&analysis)?);
        print_profile_stage("json serialization", started);
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
                let started = start_profile_timer();
                println!("{}", serde_json::to_string(&result)?);
                print_profile_stage("json serialization", started);
            } else {
                print_doctor_report(&result);
            }
            Ok(!result.warnings.is_empty() || !result.unresolved_imports.is_empty())
        }
        Command::Query(args) => run_query_command(args),
    }
}

fn run_query_command(args: QueryArgs) -> Result<bool> {
    let (kind, args) = match args.command {
        QueryCommand::Somepath(args) => (codescythe::QueryKind::Somepath, args),
        QueryCommand::Somepaths(args) => (codescythe::QueryKind::Somepaths, args),
        QueryCommand::Allpaths(args) => (codescythe::QueryKind::Allpaths, args),
    };
    let config = args.config.as_deref();
    let cwd = analysis_root(args.directory.as_deref(), config)?;
    let result = codescythe::query(
        &cwd,
        config,
        codescythe::QueryRequest {
            kind,
            from: args.from,
            to: args.to,
        },
    )?;
    let output = if args.json {
        QueryOutputFormat::Json
    } else {
        args.output
    };
    match output {
        QueryOutputFormat::Json => {
            let started = start_profile_timer();
            println!("{}", serde_json::to_string(&result)?);
            print_profile_stage("json serialization", started);
        }
        QueryOutputFormat::Mermaid => {
            print!("{}", codescythe::render_query_mermaid(&result));
        }
        QueryOutputFormat::Text => {
            print_query_report(&result);
        }
    }
    Ok(false)
}

#[cfg(feature = "profiling")]
struct CliProfileTimer(Option<Instant>);

#[cfg(not(feature = "profiling"))]
struct CliProfileTimer;

#[cfg(feature = "profiling")]
fn start_profile_timer() -> CliProfileTimer {
    CliProfileTimer(profile_enabled().then(Instant::now))
}

#[cfg(not(feature = "profiling"))]
fn start_profile_timer() -> CliProfileTimer {
    CliProfileTimer
}

#[cfg(feature = "profiling")]
fn print_profile_stage(name: &str, started: CliProfileTimer) {
    let Some(started) = started.0 else {
        return;
    };
    eprintln!("codescythe cli profile:");
    eprintln!("  {name}: {}", format_duration(started.elapsed()));
}

#[cfg(not(feature = "profiling"))]
fn print_profile_stage(_name: &str, _started: CliProfileTimer) {}

#[cfg(feature = "profiling")]
fn profile_enabled() -> bool {
    env::var(PROFILE_ENV)
        .ok()
        .is_some_and(|value| !matches!(value.as_str(), "" | "0" | "false" | "FALSE"))
}

#[cfg(feature = "profiling")]
fn format_duration(duration: Duration) -> String {
    let millis = duration.as_secs_f64() * 1000.0;
    if millis >= 1000.0 {
        format!("{:.2}s", millis / 1000.0)
    } else {
        format!("{millis:.1}ms")
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

fn reason_text(reason: &codescythe::ExplanationReason) -> String {
    if let Some(detail) = &reason.detail {
        format!("{} [{}]: {}", reason.description, reason.code, detail)
    } else {
        format!("{} [{}]", reason.description, reason.code)
    }
}

fn print_explain_export(analysis: &codescythe::Analysis) {
    let Some(result) = &analysis.explain_export else {
        return;
    };
    println!(
        "{}:{} is {:?}: {}",
        result.exporting_file,
        result.symbol,
        result.status,
        reason_text(&result.reason)
    );
    if let Some(explanation) = &result.explanation {
        if explanation.internal {
            println!("  internal: true");
        }
        println!("  file reachable: {}", explanation.file_reachable);
        if !explanation.importers_considered.is_empty() {
            println!("  importers considered:");
            for importer in &explanation.importers_considered {
                println!(
                    "    {} imports {} ({})",
                    importer.importer,
                    importer.specifier,
                    reason_text(&importer.reason)
                );
            }
        }
        if !explanation.importers_skipped.is_empty() {
            println!("  importers skipped:");
            for importer in &explanation.importers_skipped {
                println!(
                    "    {} imports {} ({})",
                    importer.importer,
                    importer.specifier,
                    reason_text(&importer.reason)
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

fn print_query_report(result: &codescythe::QueryResult) {
    match result.kind {
        codescythe::QueryKind::Somepath | codescythe::QueryKind::Somepaths => {
            if result.paths.is_empty() {
                println!(
                    "No path found from {} to {}",
                    result.from.raw, result.to.raw
                );
                return;
            }

            for (index, path) in result.paths.iter().enumerate() {
                if result.paths.len() > 1 {
                    println!("Path {}:", index + 1);
                }
                print_query_path(path);
            }
        }
        codescythe::QueryKind::Allpaths => {
            let Some(graph) = &result.graph else {
                println!("Path graph: 0 nodes, 0 edges");
                return;
            };
            println!(
                "Path graph: {} nodes, {} edges",
                graph.nodes.len(),
                graph.edges.len()
            );
            let node_by_id = graph
                .nodes
                .iter()
                .map(|node| (node.id.as_str(), node))
                .collect::<std::collections::BTreeMap<_, _>>();
            for edge in &graph.edges {
                let from = node_by_id
                    .get(edge.from.as_str())
                    .map(|node| query_node_label(node))
                    .unwrap_or_else(|| edge.from.clone());
                let to = node_by_id
                    .get(edge.to.as_str())
                    .map(|node| query_node_label(node))
                    .unwrap_or_else(|| edge.to.clone());
                println!("  {} -- {} -> {}", from, query_edge_label(edge), to);
            }
        }
    }
}

fn print_query_path(path: &codescythe::QueryPath) {
    if let Some(first) = path.nodes.first() {
        println!("  {}", query_node_label(first));
    }
    for (edge, node) in path.edges.iter().zip(path.nodes.iter().skip(1)) {
        println!(
            "  -- {} -> {}",
            query_edge_label(edge),
            query_node_label(node)
        );
    }
}

fn query_node_label(node: &codescythe::QueryNode) -> String {
    if let Some(symbol) = &node.symbol {
        format!("{}:{symbol}", node.path)
    } else {
        node.path.clone()
    }
}

fn query_edge_label(edge: &codescythe::QueryEdge) -> String {
    let kind = match edge.kind {
        codescythe::QueryEdgeKind::NamedImport => "named import",
        codescythe::QueryEdgeKind::SideEffectImport => "side-effect import",
        codescythe::QueryEdgeKind::DynamicImport => "dynamic import",
        codescythe::QueryEdgeKind::GlobImport => "glob import",
        codescythe::QueryEdgeKind::ReExport => "re-export",
        codescythe::QueryEdgeKind::ReExportSource => "re-export source",
        codescythe::QueryEdgeKind::NamespaceExport => "namespace export",
        codescythe::QueryEdgeKind::NamespaceMember => "namespace member",
        codescythe::QueryEdgeKind::ExportDefinition => "defined in file",
    };
    match (&edge.specifier, &edge.imported) {
        (Some(specifier), Some(imported)) => format!("{kind} {specifier}:{imported}"),
        (Some(specifier), None) => format!("{kind} {specifier}"),
        (None, Some(imported)) => format!("{kind} {imported}"),
        (None, None) => kind.to_string(),
    }
}

fn print_doctor_report(result: &codescythe::ConfigDoctorResult) {
    if result.warnings.is_empty()
        && result.unresolved_imports.is_empty()
        && result.internal_exports_used_by_tests.is_empty()
    {
        println!("No risky Codescythe config found");
        return;
    }

    if !result.warnings.is_empty() {
        println!("Config warnings ({})", result.warnings.len());
        for warning in &result.warnings {
            println!("  {}: {}", warning.code, warning.message);
        }
    }

    if !result.unresolved_imports.is_empty() {
        println!(
            "Unresolved import diagnostics ({})",
            result.unresolved_imports.len()
        );
        for unresolved in &result.unresolved_imports {
            println!("  {}: {}", unresolved.importer, unresolved.specifier);
            println!("    resolver error: {}", unresolved.resolver_error);
            for alias in &unresolved.matched_aliases {
                println!(
                    "    alias {} from {} via {} -> {}",
                    alias.key, alias.source, alias.target, alias.expanded_target
                );
                for candidate in &alias.candidate_files {
                    println!(
                        "      {} exists={} inProject={}",
                        candidate.path, candidate.exists, candidate.in_project
                    );
                }
            }
        }
    }

    if !result.internal_exports_used_by_tests.is_empty() {
        println!(
            "@internal exports kept alive by tests ({})",
            result.internal_exports_used_by_tests.len()
        );
        for usage in &result.internal_exports_used_by_tests {
            println!("  {}:{}", usage.exporting_file, usage.symbol);
            for importer in &usage.test_importers {
                println!(
                    "    {} imports {} ({})",
                    importer.importer,
                    importer.specifier,
                    reason_text(&importer.reason)
                );
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
