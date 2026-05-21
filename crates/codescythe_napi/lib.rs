use std::path::{Component, Path, PathBuf};

use napi::Error;
use napi_derive::napi;

#[napi(object)]
pub struct RunOptions {
    pub cwd: Option<String>,
    pub config: Option<String>,
    pub fix: Option<bool>,
    pub json: Option<bool>,
    pub verbose: Option<bool>,
}

#[napi]
pub fn analyze(options: Option<RunOptions>) -> napi::Result<String> {
    let options = options.unwrap_or_default();
    let config = options.config.as_deref().map(PathBuf::from);
    let cwd = cwd(options.cwd, config.as_deref())?;
    let loaded_config =
        codescythe::load_config_with_source(&cwd, config.as_deref()).map_err(to_napi_error)?;
    let mut analysis = codescythe::analyze_path(
        &cwd,
        &loaded_config.config,
        codescythe::AnalysisOptions {
            diagnostics: options.verbose.unwrap_or(false),
            ..codescythe::AnalysisOptions::default()
        },
    )
    .map_err(to_napi_error)?;
    attach_runtime_diagnostics(
        &mut analysis,
        &cwd,
        &loaded_config.source,
        false,
        options.json.unwrap_or(false),
        options.verbose.unwrap_or(false),
    )?;
    serde_json::to_string(&analysis).map_err(to_napi_error)
}

#[napi]
pub fn fix(options: Option<RunOptions>) -> napi::Result<String> {
    let options = options.unwrap_or_default();
    let config = options.config.as_deref().map(PathBuf::from);
    let cwd = cwd(options.cwd, config.as_deref())?;
    let loaded_config =
        codescythe::load_config_with_source(&cwd, config.as_deref()).map_err(to_napi_error)?;
    let analysis = codescythe::analyze_path(
        &cwd,
        &loaded_config.config,
        codescythe::AnalysisOptions {
            diagnostics: options.verbose.unwrap_or(false),
            ..codescythe::AnalysisOptions::default()
        },
    )
    .map_err(to_napi_error)?;
    let mut result = codescythe::apply_fixes(&cwd, &analysis).map_err(to_napi_error)?;
    let fix_plan = codescythe::fix_plan_diagnostics(&analysis, &result);
    if let Some(diagnostics) = result.analysis.diagnostics.as_mut() {
        diagnostics.fix_plan = Some(fix_plan);
    }
    attach_runtime_diagnostics(
        &mut result.analysis,
        &cwd,
        &loaded_config.source,
        true,
        options.json.unwrap_or(false),
        options.verbose.unwrap_or(false),
    )?;
    serde_json::to_string(&result).map_err(to_napi_error)
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            cwd: None,
            config: None,
            fix: None,
            json: None,
            verbose: None,
        }
    }
}

fn attach_runtime_diagnostics(
    analysis: &mut codescythe::Analysis,
    cwd: &std::path::Path,
    config_source: &codescythe::ConfigSource,
    fix: bool,
    json: bool,
    verbose: bool,
) -> napi::Result<()> {
    let Some(diagnostics) = analysis.diagnostics.as_mut() else {
        return Ok(());
    };
    diagnostics.runtime = Some(codescythe::RuntimeDiagnostics {
        version: format!("codescythe {}", env!("CARGO_PKG_VERSION")),
        process_cwd: std::env::current_dir()
            .map_err(to_napi_error)?
            .to_string_lossy()
            .replace('\\', "/"),
        resolved_directory: display_path(cwd)?,
        config_source: codescythe::RuntimeConfigSource {
            kind: config_source_kind(config_source.kind).to_string(),
            path: config_source
                .path
                .as_ref()
                .map(|path| display_path(path))
                .transpose()?,
        },
        fix,
        json,
        verbose,
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

fn display_path(path: &Path) -> napi::Result<String> {
    Ok(absolute_normalize_path(path)?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn absolute_normalize_path(path: &Path) -> napi::Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(to_napi_error)?.join(path)
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

fn cwd(value: Option<String>, config: Option<&Path>) -> napi::Result<PathBuf> {
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
