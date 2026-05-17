use std::path::PathBuf;

use napi::Error;
use napi_derive::napi;

#[napi(object)]
pub struct RunOptions {
    pub cwd: Option<String>,
    pub config: Option<String>,
    pub fix: Option<bool>,
}

#[napi]
pub fn analyze(options: Option<RunOptions>) -> napi::Result<String> {
    let options = options.unwrap_or_default();
    let cwd = cwd(options.cwd)?;
    let config = options.config.as_deref().map(PathBuf::from);
    let analysis = codescythe::run(&cwd, config.as_deref()).map_err(to_napi_error)?;
    serde_json::to_string(&analysis).map_err(to_napi_error)
}

#[napi]
pub fn fix(options: Option<RunOptions>) -> napi::Result<String> {
    let options = options.unwrap_or_default();
    let cwd = cwd(options.cwd)?;
    let config = options.config.as_deref().map(PathBuf::from);
    let result = codescythe::run_and_fix(&cwd, config.as_deref()).map_err(to_napi_error)?;
    serde_json::to_string(&result).map_err(to_napi_error)
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            cwd: None,
            config: None,
            fix: None,
        }
    }
}

fn cwd(value: Option<String>) -> napi::Result<PathBuf> {
    match value {
        Some(path) => Ok(PathBuf::from(path)),
        None => std::env::current_dir().map_err(to_napi_error),
    }
}

fn to_napi_error(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
}
