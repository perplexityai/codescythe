use std::path::PathBuf;

use napi::Error;
use napi_derive::napi;

#[napi(object)]
pub struct RunOptions {
    pub cwd: Option<String>,
    pub config: Option<String>,
    pub fix: Option<bool>,
    pub verbose: Option<bool>,
    pub force: Option<bool>,
    pub explain_export: Option<String>,
}

#[napi]
pub fn analyze(options: Option<RunOptions>) -> napi::Result<String> {
    let options = options.unwrap_or_default();
    let config = options.config.as_deref().map(PathBuf::from);
    let cwd = cwd(options.cwd, config.as_deref())?;
    let explain_export = options
        .explain_export
        .as_deref()
        .map(parse_explain_export)
        .transpose()
        .map_err(to_napi_error)?;
    let analysis = codescythe::run_with_options(
        &cwd,
        config.as_deref(),
        codescythe::AnalysisOptions {
            verbose: options.verbose.unwrap_or(false) || explain_export.is_some(),
            explain_export,
            ..codescythe::AnalysisOptions::default()
        },
    )
    .map_err(to_napi_error)?;
    serde_json::to_string(&analysis).map_err(to_napi_error)
}

#[napi]
pub fn fix(options: Option<RunOptions>) -> napi::Result<String> {
    let options = options.unwrap_or_default();
    let config = options.config.as_deref().map(PathBuf::from);
    let cwd = cwd(options.cwd, config.as_deref())?;
    let result = codescythe::run_and_fix_with_options(
        &cwd,
        config.as_deref(),
        codescythe::FixOptions {
            verbose: options.verbose.unwrap_or(false),
            force: options.force.unwrap_or(false),
        },
    )
    .map_err(to_napi_error)?;
    serde_json::to_string(&result).map_err(to_napi_error)
}

#[napi]
pub fn doctor(options: Option<RunOptions>) -> napi::Result<String> {
    let options = options.unwrap_or_default();
    let config = options.config.as_deref().map(PathBuf::from);
    let cwd = cwd(options.cwd, config.as_deref())?;
    let result = codescythe::doctor(&cwd, config.as_deref()).map_err(to_napi_error)?;
    serde_json::to_string(&result).map_err(to_napi_error)
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            cwd: None,
            config: None,
            fix: None,
            verbose: None,
            force: None,
            explain_export: None,
        }
    }
}

fn parse_explain_export(value: &str) -> anyhow::Result<codescythe::ExplainExportRequest> {
    let Some((file, symbol)) = value.rsplit_once(':') else {
        anyhow::bail!("explainExport must be formatted as <file>:<symbol>");
    };
    Ok(codescythe::ExplainExportRequest {
        file: file.to_string(),
        symbol: symbol.to_string(),
    })
}

fn cwd(value: Option<String>, config: Option<&std::path::Path>) -> napi::Result<PathBuf> {
    match value {
        Some(path) => Ok(PathBuf::from(path)),
        None => config
            .and_then(std::path::Path::parent)
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(|| std::env::current_dir().map_err(to_napi_error)),
    }
}

fn to_napi_error(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
}
