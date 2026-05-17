use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const SCHEMA: &str = include_str!("../../../codescythe.schema.json");

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CodescytheConfig {
    #[serde(deserialize_with = "deserialize_patterns")]
    pub entry: Vec<String>,
    #[serde(deserialize_with = "deserialize_patterns")]
    pub project: Vec<String>,
    #[serde(deserialize_with = "deserialize_patterns")]
    pub ignore: Vec<String>,
    pub include_entry_exports: bool,
    pub ignore_exports_used_in_file: bool,
}

pub fn load_config(cwd: &Path, config_path: Option<&Path>) -> Result<CodescytheConfig> {
    let value = match config_path {
        Some(path) => Some(read_config_file(path)?),
        None => {
            let codescythe_json = cwd.join("codescythe.json");
            if codescythe_json.exists() {
                Some(read_config_file(&codescythe_json)?)
            } else {
                let package_json = cwd.join("package.json");
                if package_json.exists() {
                    let package_value = read_json_file(&package_json)?;
                    package_value.get("codescythe").cloned()
                } else {
                    None
                }
            }
        }
    };

    let mut config = match value {
        Some(value) => {
            validate_config_value(&value)?;
            serde_json::from_value::<CodescytheConfig>(value)
                .context("failed to deserialize Codescythe configuration")?
        }
        None => CodescytheConfig::default(),
    };

    if config.project.is_empty() {
        config.project = vec!["**/*.{ts,tsx,js,jsx,mts,cts}".to_string()];
    }

    config.ignore.extend([
        ".git/**".to_string(),
        "bazel-*/**".to_string(),
        "node_modules/**".to_string(),
        "dist/**".to_string(),
        "build/**".to_string(),
        "coverage/**".to_string(),
    ]);

    Ok(config)
}

fn read_config_file(path: &Path) -> Result<Value> {
    let value = read_json_file(path)?;
    if path.file_name().and_then(|name| name.to_str()) == Some("package.json") {
        value
            .get("codescythe")
            .cloned()
            .context("package.json does not contain a codescythe config object")
    } else {
        Ok(value)
    }
}

fn read_json_file(path: &Path) -> Result<Value> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    serde_json::from_str(&source)
        .with_context(|| format!("failed to parse JSON config file {}", path.display()))
}

fn validate_config_value(value: &Value) -> Result<()> {
    let schema: Value =
        serde_json::from_str(SCHEMA).context("bundled config schema is invalid JSON")?;
    let validator =
        jsonschema::validator_for(&schema).context("bundled config schema is invalid")?;
    let errors = validator
        .iter_errors(value)
        .map(|error| format!("{}: {}", error.instance_path(), error))
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        bail!("invalid Codescythe configuration:\n{}", errors.join("\n"));
    }
    Ok(())
}

fn deserialize_patterns<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<StringOrVec>::deserialize(deserializer)?;
    Ok(match value {
        Some(StringOrVec::String(value)) => vec![value],
        Some(StringOrVec::Vec(values)) => values,
        None => Vec::new(),
    })
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StringOrVec {
    String(String),
    Vec(Vec<String>),
}
