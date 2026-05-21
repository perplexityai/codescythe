use super::*;

pub(super) fn discover_project_files(
    cwd: &Path,
    config: &CodescytheConfig,
) -> Result<Vec<PathBuf>> {
    let include = build_glob_set(&config.project)?;
    let ignore = Arc::new(build_glob_set(&config.ignore)?);
    let mut files = Vec::new();

    let mut walker = WalkBuilder::new(cwd);
    walker
        .follow_links(true)
        .standard_filters(false)
        .git_ignore(true)
        .require_git(false);

    let filter_cwd = cwd.to_path_buf();
    let filter_ignore = Arc::clone(&ignore);
    walker.filter_entry(move |entry| should_enter(&filter_cwd, entry, &filter_ignore));

    for entry in walker.build() {
        let entry = entry?;
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        let relative = relative_path(cwd, entry.path());
        if include.is_match(&relative) && !ignore.is_match(&relative) {
            files.push(entry.path().to_path_buf());
        }
    }

    files.sort();
    Ok(files)
}

pub(super) fn discover_test_file_indexes(
    cwd: &Path,
    config: &CodescytheConfig,
    project_files: &[PathBuf],
) -> Result<TestFiles> {
    let test = build_glob_set(&config.test_file_patterns)?;
    Ok(project_files
        .iter()
        .enumerate()
        .filter_map(|(index, path)| test.is_match(relative_path(cwd, path)).then_some(index))
        .collect())
}

pub(super) fn discover_entry_files(
    cwd: &Path,
    config: &CodescytheConfig,
    project_files: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    if config.entry.is_empty() {
        let inferred = infer_entry_files(cwd)?;
        return Ok(inferred
            .into_iter()
            .filter(|path| project_files.contains(path))
            .collect());
    }

    let entry_globs = build_glob_set(&config.entry)?;
    let mut entries = BTreeSet::<PathBuf>::new();
    for pattern in &config.entry {
        if !has_glob_meta(pattern) {
            let path = normalize_path(&cwd.join(pattern));
            if path.exists() {
                entries.insert(path);
            }
        }
    }
    for file in project_files {
        if entry_globs.is_match(relative_path(cwd, file)) {
            entries.insert(file.clone());
        }
    }
    Ok(entries.into_iter().collect())
}

fn infer_entry_files(cwd: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = BTreeSet::<PathBuf>::new();
    for candidate in [
        "src/index.ts",
        "src/index.tsx",
        "src/index.js",
        "index.ts",
        "index.tsx",
        "index.js",
    ] {
        let path = cwd.join(candidate);
        if path.exists() {
            entries.insert(normalize_path(&path));
        }
    }

    let package_json = cwd.join("package.json");
    if package_json.exists() {
        let value = serde_json::from_str::<serde_json::Value>(&fs::read_to_string(package_json)?)?;
        for field in ["main", "module", "types"] {
            if let Some(path) = value.get(field).and_then(|value| value.as_str()) {
                let path = cwd.join(path);
                if path.exists() {
                    entries.insert(normalize_path(&path));
                }
            }
        }
        if let Some(bin) = value.get("bin") {
            match bin {
                serde_json::Value::String(path) => {
                    let path = cwd.join(path);
                    if path.exists() {
                        entries.insert(normalize_path(&path));
                    }
                }
                serde_json::Value::Object(map) => {
                    for path in map.values().filter_map(|value| value.as_str()) {
                        let path = cwd.join(path);
                        if path.exists() {
                            entries.insert(normalize_path(&path));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(entries.into_iter().collect())
}
