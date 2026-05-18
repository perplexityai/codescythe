mod analyze;
mod config;
mod fix;

pub use analyze::{
    Analysis, AnalysisOptions, Counters, FileIssue, Issues, SymbolIssue, analyze_path,
};
pub use config::{CodescytheConfig, UnresolvedImportsMode, load_config};
pub use fix::{FixResult, apply_fixes};

use std::path::Path;

pub fn run(cwd: impl AsRef<Path>, config_path: Option<&Path>) -> anyhow::Result<Analysis> {
    let cwd = cwd.as_ref();
    let config = load_config(cwd, config_path)?;
    analyze_path(cwd, &config, AnalysisOptions::default())
}

pub fn run_and_fix(cwd: impl AsRef<Path>, config_path: Option<&Path>) -> anyhow::Result<FixResult> {
    let cwd = cwd.as_ref();
    let config = load_config(cwd, config_path)?;
    let analysis = analyze_path(cwd, &config, AnalysisOptions::default())?;
    apply_fixes(cwd, &analysis)
}
