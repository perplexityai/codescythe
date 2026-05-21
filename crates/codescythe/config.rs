use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const SCHEMA: &str = include_str!("../../codescythe.schema.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CodescytheConfig {
    #[serde(deserialize_with = "deserialize_patterns")]
    pub entry: Vec<String>,
    #[serde(deserialize_with = "deserialize_patterns")]
    pub project: Vec<String>,
    #[serde(
        default = "default_test_file_patterns",
        deserialize_with = "deserialize_patterns"
    )]
    pub test_file_patterns: Vec<String>,
    #[serde(deserialize_with = "deserialize_patterns")]
    pub ignore: Vec<String>,
    #[serde(deserialize_with = "deserialize_aliases")]
    pub aliases: BTreeMap<String, Vec<String>>,
    pub unresolved_imports: UnresolvedImportsConfig,
    pub include_entry_exports: bool,
    pub ignore_exports_used_in_file: bool,
}

impl Default for CodescytheConfig {
    fn default() -> Self {
        Self {
            entry: Vec::new(),
            project: Vec::new(),
            test_file_patterns: default_test_file_patterns(),
            ignore: Vec::new(),
            aliases: BTreeMap::new(),
            unresolved_imports: UnresolvedImportsConfig::default(),
            include_entry_exports: false,
            ignore_exports_used_in_file: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct UnresolvedImportsConfig {
    pub mode: UnresolvedImportsMode,
    #[serde(deserialize_with = "deserialize_patterns")]
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UnresolvedImportsMode {
    #[default]
    Report,
    Ignore,
    Error,
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: CodescytheConfig,
    pub path: Option<std::path::PathBuf>,
}

pub fn load_config(cwd: &Path, config_path: Option<&Path>) -> Result<CodescytheConfig> {
    Ok(load_config_with_metadata(cwd, config_path)?.config)
}

pub fn load_config_with_metadata(cwd: &Path, config_path: Option<&Path>) -> Result<LoadedConfig> {
    let (value, path) = match config_path {
        Some(path) => (Some(read_config_file(path)?), Some(path.to_path_buf())),
        None => {
            let codescythe_json = cwd.join("codescythe.json");
            if codescythe_json.exists() {
                (
                    Some(read_config_file(&codescythe_json)?),
                    Some(codescythe_json),
                )
            } else {
                let codescythe_jsonc = cwd.join("codescythe.jsonc");
                if codescythe_jsonc.exists() {
                    (
                        Some(read_config_file(&codescythe_jsonc)?),
                        Some(codescythe_jsonc),
                    )
                } else {
                    let package_json = cwd.join("package.json");
                    if package_json.exists() {
                        let package_value = read_json_file(&package_json)?;
                        (
                            package_value.get("codescythe").cloned(),
                            package_value
                                .get("codescythe")
                                .is_some()
                                .then_some(package_json),
                        )
                    } else {
                        (None, None)
                    }
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

    Ok(LoadedConfig { config, path })
}

fn read_config_file(path: &Path) -> Result<Value> {
    if path.file_name().and_then(|name| name.to_str()) == Some("package.json") {
        let value = read_json_file(path)?;
        value
            .get("codescythe")
            .cloned()
            .context("package.json does not contain a codescythe config object")
    } else {
        read_config_value(path)
    }
}

fn read_json_file(path: &Path) -> Result<Value> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    serde_json::from_str(&source)
        .with_context(|| format!("failed to parse JSON config file {}", path.display()))
}

fn read_config_value(path: &Path) -> Result<Value> {
    if path.extension().and_then(|extension| extension.to_str()) == Some("jsonc") {
        return read_jsonc_file(path);
    }

    read_json_file(path)
}

fn read_jsonc_file(path: &Path) -> Result<Value> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    jsonc_parser::parse_to_serde_value::<Value>(&source, &jsonc_parse_options())
        .with_context(|| format!("failed to parse JSONC config file {}", path.display()))
}

fn jsonc_parse_options() -> jsonc_parser::ParseOptions {
    jsonc_parser::ParseOptions {
        allow_comments: true,
        allow_loose_object_property_names: false,
        allow_trailing_commas: true,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    }
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

fn deserialize_aliases<'de, D>(deserializer: D) -> Result<BTreeMap<String, Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<BTreeMap<String, StringOrVec>>::deserialize(deserializer)?;
    Ok(value
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| {
            let values = match value {
                StringOrVec::String(value) => vec![value],
                StringOrVec::Vec(values) => values,
            };
            (key, values)
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StringOrVec {
    String(String),
    Vec(Vec<String>),
}

fn default_test_file_patterns() -> Vec<String> {
    vec!["**/*.test.*".to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn discovers_codescythe_jsonc_config() {
        let tempdir = tempfile::tempdir().unwrap();
        write_file(
            tempdir.path(),
            "codescythe.jsonc",
            r#"{
              // JSONC config is allowed for project-local settings.
              "entry": "src/index.ts",
              "project": "src/**/*.ts",
            }"#,
        );

        let config = load_config(tempdir.path(), None).unwrap();

        assert_eq!(config.entry, vec!["src/index.ts"]);
        assert_eq!(config.project, vec!["src/**/*.ts"]);
    }

    #[test]
    fn explicit_jsonc_config_path_allows_comments_and_trailing_commas() {
        let tempdir = tempfile::tempdir().unwrap();
        let config_path = tempdir.path().join("custom.jsonc");
        write_file(
            tempdir.path(),
            "custom.jsonc",
            r#"{
              "entry": ["app/main.ts"],
              /* Keep this broad because fixtures generate nested source. */
              "project": ["app/**/*.ts"],
              "ignore": [
                "app/generated/**",
              ],
            }"#,
        );

        let config = load_config(tempdir.path(), Some(&config_path)).unwrap();

        assert_eq!(config.entry, vec!["app/main.ts"]);
        assert_eq!(config.project, vec!["app/**/*.ts"]);
        assert!(config.ignore.contains(&"app/generated/**".to_string()));
    }

    #[test]
    fn codescythe_json_takes_precedence_over_jsonc() {
        let tempdir = tempfile::tempdir().unwrap();
        write_file(
            tempdir.path(),
            "codescythe.json",
            r#"{
              "entry": "json.ts"
            }"#,
        );
        write_file(
            tempdir.path(),
            "codescythe.jsonc",
            r#"{
              "entry": "jsonc.ts"
            }"#,
        );

        let config = load_config(tempdir.path(), None).unwrap();

        assert_eq!(config.entry, vec!["json.ts"]);
    }

    #[test]
    fn validates_jsonc_config_against_schema() {
        let tempdir = tempfile::tempdir().unwrap();
        write_file(
            tempdir.path(),
            "codescythe.jsonc",
            r#"{
              // Unknown fields should still be rejected after JSONC parsing.
              "entry": "src/index.ts",
              "unknown": true,
            }"#,
        );

        let error = load_config(tempdir.path(), None).unwrap_err();
        let message = format!("{error:#}");

        assert!(message.contains("invalid Codescythe configuration"));
        assert!(message.contains("unknown"));
    }

    #[test]
    fn defaults_test_file_patterns() {
        let tempdir = tempfile::tempdir().unwrap();

        let config = load_config(tempdir.path(), None).unwrap();

        assert_eq!(config.test_file_patterns, vec!["**/*.test.*".to_string()]);
    }

    #[test]
    fn explicit_empty_test_file_patterns_disable_test_classification() {
        let tempdir = tempfile::tempdir().unwrap();
        write_file(
            tempdir.path(),
            "codescythe.json",
            r#"{
              "testFilePatterns": []
            }"#,
        );

        let config = load_config(tempdir.path(), None).unwrap();

        assert!(config.test_file_patterns.is_empty());
    }

    fn write_file(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }
}
