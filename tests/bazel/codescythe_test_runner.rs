use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

const SEPARATOR: &str = "||CODESCYTHE_TEST_SEP||";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let tool = resolve_runfile(&env_var("CODESCYTHE_TEST_TOOL")?);
    let args = env_list("CODESCYTHE_TEST_ARGS")
        .into_iter()
        .map(|arg| resolve_runfile(&arg))
        .collect::<Vec<_>>();
    let expected_exit_code = env_var("CODESCYTHE_TEST_EXPECTED_EXIT_CODE")?
        .parse::<i32>()
        .map_err(|error| format!("invalid CODESCYTHE_TEST_EXPECTED_EXIT_CODE: {error}"))?;
    let must_contain = env_list("CODESCYTHE_TEST_MUST_CONTAIN");
    let must_not_contain = env_list("CODESCYTHE_TEST_MUST_NOT_CONTAIN");

    let output = Command::new(&tool)
        .args(&args)
        .output()
        .map_err(|error| format!("failed to run {tool}: {error}"))?;
    let status = output.status.code().unwrap_or(128);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if status != expected_exit_code {
        return Err(format!(
            "expected exit code {expected_exit_code}, got {status}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        ));
    }

    if !stderr.is_empty() {
        return Err(format!("expected empty stderr, got:\n{stderr}"));
    }

    for needle in must_contain {
        if !stdout.contains(&needle) {
            return Err(format!(
                "expected Codescythe output to contain: {needle}\nstdout:\n{stdout}"
            ));
        }
    }

    for needle in must_not_contain {
        if stdout.contains(&needle) {
            return Err(format!(
                "expected Codescythe output not to contain: {needle}\nstdout:\n{stdout}"
            ));
        }
    }

    Ok(())
}

fn env_var(name: &str) -> Result<String, String> {
    env::var(name).map_err(|error| format!("missing {name}: {error}"))
}

fn env_list(name: &str) -> Vec<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .map(|value| value.split(SEPARATOR).map(str::to_owned).collect())
        .unwrap_or_default()
}

fn resolve_runfile(value: &str) -> String {
    if Path::new(value).exists() {
        return value.to_owned();
    }

    for candidate in runfile_candidates(value) {
        if candidate.exists() {
            return candidate.to_string_lossy().into_owned();
        }
    }

    manifest_lookup(value).unwrap_or_else(|| value.to_owned())
}

fn runfile_candidates(value: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for env_name in ["TEST_SRCDIR", "RUNFILES_DIR"] {
        let Ok(runfiles_root) = env::var(env_name) else {
            continue;
        };
        if let Ok(workspace) = env::var("TEST_WORKSPACE") {
            if let Some(package) = test_package() {
                candidates.push(
                    Path::new(&runfiles_root)
                        .join(&workspace)
                        .join(package)
                        .join(value),
                );
            }
            candidates.push(Path::new(&runfiles_root).join(&workspace).join(value));
        }
        candidates.push(Path::new(&runfiles_root).join(value));
    }
    candidates
}

fn manifest_lookup(value: &str) -> Option<String> {
    let manifest_file = env::var("RUNFILES_MANIFEST_FILE").ok()?;
    let manifest = fs::read_to_string(manifest_file).ok()?;
    let workspace_key = env::var("TEST_WORKSPACE")
        .ok()
        .map(|workspace| format!("{workspace}/{value}"));
    let package_key = env::var("TEST_WORKSPACE")
        .ok()
        .zip(test_package())
        .map(|(workspace, package)| format!("{workspace}/{package}/{value}"));

    for line in manifest.lines() {
        let Some((key, path)) = line.split_once(' ') else {
            continue;
        };
        if key == value || workspace_key.as_deref() == Some(key) || package_key.as_deref() == Some(key)
        {
            return Some(path.to_owned());
        }
    }

    None
}

fn test_package() -> Option<String> {
    env::var("CODESCYTHE_TEST_PACKAGE")
        .ok()
        .filter(|package| !package.is_empty())
}
