use super::*;

pub(super) fn build_glob_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder
            .add(Glob::new(pattern).with_context(|| format!("invalid glob pattern {pattern:?}"))?);
    }
    Ok(builder.build()?)
}

pub(super) fn should_enter(cwd: &Path, entry: &DirEntry, ignore: &GlobSet) -> bool {
    if !entry
        .file_type()
        .is_some_and(|file_type| file_type.is_dir())
    {
        return true;
    }
    if ignore.is_match(relative_path(cwd, entry.path())) {
        return false;
    }
    !matches!(
        entry.file_name().to_str(),
        Some(".git" | "node_modules" | "target" | "dist" | "build" | "coverage")
    ) && !entry.file_name().to_string_lossy().starts_with("bazel-")
}

pub(super) fn project_glob_from_import(file_relative: &str, pattern: &str) -> Option<String> {
    if pattern.starts_with('!') {
        return None;
    }

    let path = if let Some(pattern) = pattern.strip_prefix('/') {
        PathBuf::from(pattern)
    } else if is_relative_alias_path(pattern) {
        Path::new(file_relative)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(pattern)
    } else {
        return None;
    };

    Some(normalize_path(&path).to_string_lossy().replace('\\', "/"))
}

pub(super) fn relative_path(cwd: &Path, path: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(super) fn absolute_normalize_path(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    Ok(normalize_path(&path))
}

pub(super) fn normalize_path(path: &Path) -> PathBuf {
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

pub(super) fn has_glob_meta(pattern: &str) -> bool {
    pattern
        .bytes()
        .any(|byte| matches!(byte, b'*' | b'?' | b'[' | b'{'))
}

pub(super) fn is_relative_alias_path(value: &str) -> bool {
    value == "." || value == ".." || value.starts_with("./") || value.starts_with("../")
}
