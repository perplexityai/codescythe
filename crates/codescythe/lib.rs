mod analyze;
mod config;
mod fix;

pub use analyze::{
    Analysis, AnalysisOptions, AnalysisSummary, ConfigDoctorResult, ConfigDoctorWarning, Counters,
    ExplainExportRequest, ExplainExportResult, ExplainExportStatus, ExportExplanation, FileIssue,
    IgnoredUnresolvedImportSample, IgnoredUnresolvedImportsByPattern, Issues,
    SourceAliasIgnoreWarning, SymbolIssue, analyze_path, doctor_config,
    source_alias_fix_blocking_ignore_warnings_for_config, source_alias_ignore_warnings_for_config,
};
pub use config::{
    CodescytheConfig, LoadedConfig, UnresolvedImportsConfig, UnresolvedImportsMode, load_config,
    load_config_with_metadata,
};
pub use fix::{FixResult, apply_fixes, apply_fixes_with_options};

use std::path::Path;

#[derive(Debug, Clone, Copy, Default)]
pub struct FixOptions {
    pub verbose: bool,
    pub force: bool,
}

pub fn run(cwd: impl AsRef<Path>, config_path: Option<&Path>) -> anyhow::Result<Analysis> {
    run_with_options(cwd, config_path, AnalysisOptions::default())
}

pub fn run_with_options(
    cwd: impl AsRef<Path>,
    config_path: Option<&Path>,
    options: AnalysisOptions,
) -> anyhow::Result<Analysis> {
    let cwd = cwd.as_ref();
    let loaded = load_config_with_metadata(cwd, config_path)?;
    let mut options = options;
    options.config_path = loaded.path;
    analyze_path(cwd, &loaded.config, options)
}

pub fn run_and_fix(cwd: impl AsRef<Path>, config_path: Option<&Path>) -> anyhow::Result<FixResult> {
    run_and_fix_with_options(cwd, config_path, FixOptions::default())
}

pub fn run_and_fix_with_options(
    cwd: impl AsRef<Path>,
    config_path: Option<&Path>,
    options: FixOptions,
) -> anyhow::Result<FixResult> {
    let cwd = cwd.as_ref();
    let loaded = load_config_with_metadata(cwd, config_path)?;
    let source_alias_warnings =
        source_alias_fix_blocking_ignore_warnings_for_config(cwd, &loaded.config)?;
    if !options.force && !source_alias_warnings.is_empty() {
        anyhow::bail!(
            "--fix refused because unresolvedImports.ignore overlaps local source aliases; rerun with --force to override"
        );
    }
    let analysis = analyze_path(
        cwd,
        &loaded.config,
        AnalysisOptions {
            verbose: options.verbose,
            retain_ignored_unresolved: true,
            config_path: loaded.path,
            ..AnalysisOptions::default()
        },
    )?;
    apply_fixes_with_options(cwd, &loaded.config, &analysis, options.force)
}

pub fn doctor(
    cwd: impl AsRef<Path>,
    config_path: Option<&Path>,
) -> anyhow::Result<ConfigDoctorResult> {
    let cwd = cwd.as_ref();
    let loaded = load_config_with_metadata(cwd, config_path)?;
    doctor_config(cwd, &loaded.config, loaded.path.as_deref())
}
