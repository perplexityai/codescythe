use super::*;

pub(super) fn mark_member_import(
    from_file: &FileData,
    source: &str,
    member: &str,
    files: &mut FileCache,
    resolver: &ModuleResolver,
    used_files: &mut UsedFiles,
    used_exports: &mut UsedExports,
    queue: &mut VecDeque<usize>,
    queued_files: &mut QueuedFiles,
    unresolved: &mut UnresolvedImports,
    ignored: &mut BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
    unresolved_policy: &UnresolvedImportPolicy,
    importer_relative: &str,
    test_file_indexes: &TestFiles,
) -> Result<()> {
    match resolver.resolve(from_file, source)? {
        ImportResolution::Project(target) => {
            mark_used_export(
                target,
                member.to_string(),
                used_files,
                used_exports,
                queue,
                queued_files,
                test_file_indexes,
            );
            let namespace_source = files
                .get(target)?
                .exports
                .get(member)
                .and_then(|export| export.namespace_source.clone());
            if let Some(namespace_source) = namespace_source {
                let target_file = files.get(target)?.clone();
                mark_member_import(
                    &target_file,
                    &namespace_source,
                    member,
                    files,
                    resolver,
                    used_files,
                    used_exports,
                    queue,
                    queued_files,
                    unresolved,
                    ignored,
                    unresolved_policy,
                    importer_relative,
                    test_file_indexes,
                )?;
            }
        }
        ImportResolution::Unresolved => {
            unresolved_policy.record(unresolved, ignored, importer_relative, source)?;
        }
        ImportResolution::External => {}
    }
    Ok(())
}

pub(super) fn mark_used_export(
    target: usize,
    name: String,
    used_files: &mut UsedFiles,
    used_exports: &mut UsedExports,
    queue: &mut VecDeque<usize>,
    queued_files: &mut QueuedFiles,
    test_file_indexes: &TestFiles,
) {
    let file_was_new = used_files.insert(target);
    let export_was_new = used_exports.entry(target).or_default().insert(name);

    if (file_was_new || export_was_new) && !test_file_indexes.contains(&target) {
        enqueue_file(target, queue, queued_files);
    }
}

pub(super) fn mark_used_file(
    target: usize,
    test_file_indexes: &TestFiles,
    used_files: &mut UsedFiles,
    queue: &mut VecDeque<usize>,
    queued_files: &mut QueuedFiles,
) {
    if used_files.insert(target) && !test_file_indexes.contains(&target) {
        enqueue_file(target, queue, queued_files);
    }
}

fn enqueue_file(target: usize, queue: &mut VecDeque<usize>, queued_files: &mut QueuedFiles) {
    if queued_files.insert(target) {
        queue.push_back(target);
    }
}

pub(super) fn mark_reexport(
    file: &FileData,
    export: &ExportInfo,
    resolver: &ModuleResolver,
    used_files: &mut UsedFiles,
    used_exports: &mut UsedExports,
    queue: &mut VecDeque<usize>,
    queued_files: &mut QueuedFiles,
    unresolved: &mut UnresolvedImports,
    ignored: &mut BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
    unresolved_policy: &UnresolvedImportPolicy,
    test_file_indexes: &TestFiles,
) -> Result<()> {
    if let (Some(source), Some(name)) = (&export.reexport_source, &export.reexport_name) {
        match resolver.resolve(file, source)? {
            ImportResolution::Project(target) => {
                mark_used_export(
                    target,
                    name.clone(),
                    used_files,
                    used_exports,
                    queue,
                    queued_files,
                    test_file_indexes,
                );
            }
            ImportResolution::Unresolved => {
                unresolved_policy.record(unresolved, ignored, &file.relative, source)?;
            }
            ImportResolution::External => {}
        }
    }

    if let Some(source) = &export.namespace_source {
        match resolver.resolve(file, source)? {
            ImportResolution::Project(target) => {
                mark_used_file(target, test_file_indexes, used_files, queue, queued_files);
            }
            ImportResolution::Unresolved => {
                unresolved_policy.record(unresolved, ignored, &file.relative, source)?;
            }
            ImportResolution::External => {}
        }
    }
    Ok(())
}

pub(super) fn mark_reexport_source_file(
    file: &FileData,
    export: &ExportInfo,
    resolver: &ModuleResolver,
    used_files: &mut UsedFiles,
    queue: &mut VecDeque<usize>,
    queued_files: &mut QueuedFiles,
    unresolved: &mut UnresolvedImports,
    ignored: &mut BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
    unresolved_policy: &UnresolvedImportPolicy,
    test_file_indexes: &TestFiles,
) -> Result<()> {
    if let Some(source) = &export.reexport_source {
        mark_source_file(
            file,
            source,
            resolver,
            used_files,
            queue,
            queued_files,
            unresolved,
            ignored,
            unresolved_policy,
            test_file_indexes,
        )?;
    }

    if let Some(source) = &export.namespace_source {
        mark_source_file(
            file,
            source,
            resolver,
            used_files,
            queue,
            queued_files,
            unresolved,
            ignored,
            unresolved_policy,
            test_file_indexes,
        )?;
    }

    Ok(())
}

pub(super) fn mark_source_file(
    file: &FileData,
    source: &str,
    resolver: &ModuleResolver,
    used_files: &mut UsedFiles,
    queue: &mut VecDeque<usize>,
    queued_files: &mut QueuedFiles,
    unresolved: &mut UnresolvedImports,
    ignored: &mut BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
    unresolved_policy: &UnresolvedImportPolicy,
    test_file_indexes: &TestFiles,
) -> Result<()> {
    match resolver.resolve(file, source)? {
        ImportResolution::Project(target) => {
            mark_used_file(target, test_file_indexes, used_files, queue, queued_files);
        }
        ImportResolution::Unresolved => {
            unresolved_policy.record(unresolved, ignored, &file.relative, source)?;
        }
        ImportResolution::External => {}
    }
    Ok(())
}

pub(super) fn mark_glob_import(
    file: &FileData,
    pattern: &str,
    files: &mut FileCache,
    used_files: &mut UsedFiles,
    used_exports: &mut UsedExports,
    queue: &mut VecDeque<usize>,
    queued_files: &mut QueuedFiles,
    test_file_indexes: &TestFiles,
) -> Result<()> {
    let Some(pattern) = project_glob_from_import(&file.relative, pattern) else {
        return Ok(());
    };

    for target in files.matching_relative_glob(&pattern)? {
        let export_names = files
            .get(target)?
            .exports
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        if export_names.is_empty() {
            mark_used_file(target, test_file_indexes, used_files, queue, queued_files);
        }
        for name in export_names {
            mark_used_export(
                target,
                name,
                used_files,
                used_exports,
                queue,
                queued_files,
                test_file_indexes,
            );
        }
    }

    Ok(())
}

pub(super) fn mark_all_exports(
    file: &FileData,
    source: &str,
    files: &mut FileCache,
    resolver: &ModuleResolver,
    used_files: &mut UsedFiles,
    used_exports: &mut UsedExports,
    queue: &mut VecDeque<usize>,
    queued_files: &mut QueuedFiles,
    unresolved: &mut UnresolvedImports,
    ignored: &mut BTreeMap<String, IgnoredUnresolvedImportsByPattern>,
    unresolved_policy: &UnresolvedImportPolicy,
    test_file_indexes: &TestFiles,
) -> Result<()> {
    match resolver.resolve(file, source)? {
        ImportResolution::Project(target) => {
            let export_names = files
                .get(target)?
                .exports
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            if export_names.is_empty() {
                mark_used_file(target, test_file_indexes, used_files, queue, queued_files);
            }
            for name in export_names {
                mark_used_export(
                    target,
                    name,
                    used_files,
                    used_exports,
                    queue,
                    queued_files,
                    test_file_indexes,
                );
            }
        }
        ImportResolution::Unresolved => {
            unresolved_policy.record(unresolved, ignored, &file.relative, source)?;
        }
        ImportResolution::External => {}
    }
    Ok(())
}

pub(super) fn discover_live_test_support_files(
    files: &mut FileCache,
    resolver: &ModuleResolver,
    test_file_indexes: &TestFiles,
    unused_file_indexes: &HashSet<usize>,
    used_files: &UsedFiles,
) -> Result<HashSet<usize>> {
    let production_used_files = used_files
        .difference(test_file_indexes)
        .copied()
        .collect::<HashSet<_>>();
    let mut support = HashSet::<usize>::new();
    let mut queue = VecDeque::<usize>::new();
    let mut queued = HashSet::<usize>::new();

    for index in test_file_indexes {
        let file = files.get(*index)?.clone();
        if project_import_targets(&file, resolver)?
            .into_iter()
            .any(|target| production_used_files.contains(&target))
            && queued.insert(*index)
        {
            queue.push_back(*index);
        }
    }

    while let Some(index) = queue.pop_front() {
        let file = files.get(index)?.clone();
        for target in project_import_targets(&file, resolver)? {
            if production_used_files.contains(&target) {
                continue;
            }

            if test_file_indexes.contains(&target) {
                if queued.insert(target) {
                    queue.push_back(target);
                }
                continue;
            }

            if unused_file_indexes.contains(&target) && support.insert(target) {
                queue.push_back(target);
            }
        }
    }

    Ok(support)
}

fn project_import_targets(file: &FileData, resolver: &ModuleResolver) -> Result<Vec<usize>> {
    let mut targets = Vec::new();

    for import in &file.imports {
        if let ImportResolution::Project(target) = resolver.resolve(file, &import.source)? {
            targets.push(target);
        }
    }

    for source in &file.side_effect_imports {
        if let ImportResolution::Project(target) = resolver.resolve(file, source)? {
            targets.push(target);
        }
    }

    for source in &file.dynamic_imports {
        if let ImportResolution::Project(target) = resolver.resolve(file, source)? {
            targets.push(target);
        }
    }

    Ok(targets)
}

pub(super) fn discover_removable_test_files(
    files: &mut FileCache,
    resolver: &ModuleResolver,
    test_file_indexes: &TestFiles,
    unused_file_indexes: &HashSet<usize>,
    unused_exports: &BTreeMap<String, BTreeMap<String, SymbolIssue>>,
) -> Result<HashSet<usize>> {
    let mut removable = HashSet::<usize>::new();

    loop {
        let mut removed_file_indexes = unused_file_indexes.clone();
        removed_file_indexes.extend(removable.iter().copied());
        let mut changed = false;

        for index in test_file_indexes {
            if removable.contains(index) {
                continue;
            }

            let file = files.get(*index)?.clone();
            if file_references_removed_code(
                &file,
                files,
                resolver,
                &removed_file_indexes,
                unused_exports,
            )? {
                removable.insert(*index);
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }

    Ok(removable)
}

fn file_references_removed_code(
    file: &FileData,
    files: &mut FileCache,
    resolver: &ModuleResolver,
    removed_file_indexes: &HashSet<usize>,
    unused_exports: &BTreeMap<String, BTreeMap<String, SymbolIssue>>,
) -> Result<bool> {
    for import in &file.imports {
        if import_references_removed_code(
            file,
            &import.source,
            import.imported.as_deref(),
            files,
            resolver,
            removed_file_indexes,
            unused_exports,
        )? {
            return Ok(true);
        }
    }

    for source in &file.side_effect_imports {
        if import_references_removed_code(
            file,
            source,
            None,
            files,
            resolver,
            removed_file_indexes,
            unused_exports,
        )? {
            return Ok(true);
        }
    }

    for source in &file.dynamic_imports {
        if import_references_removed_code(
            file,
            source,
            None,
            files,
            resolver,
            removed_file_indexes,
            unused_exports,
        )? {
            return Ok(true);
        }
    }

    for pattern in &file.glob_imports {
        let Some(pattern) = project_glob_from_import(&file.relative, pattern) else {
            continue;
        };
        for target in files.matching_relative_glob(&pattern)? {
            if removed_file_indexes.contains(&target) {
                return Ok(true);
            }
        }
    }

    for (local, member) in &file.member_uses {
        if let Some(source) = file.namespace_imports.get(local)
            && import_references_removed_code(
                file,
                source,
                Some(member),
                files,
                resolver,
                removed_file_indexes,
                unused_exports,
            )?
        {
            return Ok(true);
        }
    }

    Ok(false)
}

fn import_references_removed_code(
    file: &FileData,
    source: &str,
    imported: Option<&str>,
    files: &FileCache,
    resolver: &ModuleResolver,
    removed_file_indexes: &HashSet<usize>,
    unused_exports: &BTreeMap<String, BTreeMap<String, SymbolIssue>>,
) -> Result<bool> {
    let ImportResolution::Project(target) = resolver.resolve(file, source)? else {
        return Ok(false);
    };

    if removed_file_indexes.contains(&target) {
        return Ok(true);
    }

    let Some(imported) = imported else {
        return Ok(false);
    };
    let target_relative = files.relative(target);
    Ok(unused_exports
        .get(&target_relative)
        .is_some_and(|exports| exports.contains_key(imported)))
}
