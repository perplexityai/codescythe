mod analyze;
mod config;
mod fix;

pub use analyze::{
    AliasDiagnostics, Analysis, AnalysisDiagnostics, AnalysisOptions, ConfigDiagnostics, Counters,
    DeadFileDiagnostics, EntryDiagnostics, FileDiscoveryDiagnostics, FileIssue, FixPlanDiagnostics,
    Issues, PackageJsonImportsDiagnostics, RuntimeConfigSource, RuntimeDiagnostics, SymbolIssue,
    UnresolvedImportsDiagnostics, analyze_path,
};
pub use config::{
    CodescytheConfig, ConfigSource, ConfigSourceKind, LoadedConfig, UnresolvedImportsConfig,
    UnresolvedImportsMode, load_config, load_config_with_source,
};
pub use fix::{FixResult, apply_fixes, fix_plan_diagnostics};

use std::path::Path;

pub fn run(cwd: impl AsRef<Path>, config_path: Option<&Path>) -> anyhow::Result<Analysis> {
    run_with_options(cwd, config_path, AnalysisOptions::default())
}

pub fn run_with_options(
    cwd: impl AsRef<Path>,
    config_path: Option<&Path>,
    options: AnalysisOptions,
) -> anyhow::Result<Analysis> {
    let cwd = cwd.as_ref();
    let config = load_config(cwd, config_path)?;
    analyze_path(cwd, &config, options)
}

pub fn run_and_fix(cwd: impl AsRef<Path>, config_path: Option<&Path>) -> anyhow::Result<FixResult> {
    run_and_fix_with_options(cwd, config_path, AnalysisOptions::default())
}

pub fn run_and_fix_with_options(
    cwd: impl AsRef<Path>,
    config_path: Option<&Path>,
    options: AnalysisOptions,
) -> anyhow::Result<FixResult> {
    let cwd = cwd.as_ref();
    let config = load_config(cwd, config_path)?;
    let analysis = analyze_path(cwd, &config, options)?;
    let mut result = apply_fixes(cwd, &analysis)?;
    let fix_plan = fix_plan_diagnostics(&analysis, &result);
    if let Some(diagnostics) = result.analysis.diagnostics.as_mut() {
        diagnostics.fix_plan = Some(fix_plan);
    }
    Ok(result)
}
