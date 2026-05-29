use super::*;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum QueryKind {
    Somepath,
    Somepaths,
    Allpaths,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QueryRequest {
    pub kind: QueryKind,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub kind: QueryKind,
    pub from: QuerySelector,
    pub to: QuerySelector,
    pub source_nodes: Vec<QueryNode>,
    pub target_nodes: Vec<QueryNode>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<QueryPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph: Option<QueryGraph>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<QueryUnresolvedImport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuerySelector {
    pub raw: String,
    pub kind: QuerySelectorKind,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum QuerySelectorKind {
    File,
    Directory,
    Export,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QueryPath {
    pub nodes: Vec<QueryNode>,
    pub edges: Vec<QueryEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QueryGraph {
    pub nodes: Vec<QueryNode>,
    pub edges: Vec<QueryEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct QueryNode {
    pub id: String,
    pub kind: QueryNodeKind,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum QueryNodeKind {
    File,
    Export,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct QueryEdge {
    pub from: String,
    pub to: String,
    pub kind: QueryEdgeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub specifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum QueryEdgeKind {
    NamedImport,
    SideEffectImport,
    DynamicImport,
    GlobImport,
    ReExport,
    ReExportSource,
    NamespaceExport,
    NamespaceMember,
    ExportDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct QueryUnresolvedImport {
    pub importer: String,
    pub specifier: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
enum QueryNodeKey {
    File(String),
    Export(String, String),
}

impl QueryNodeKey {
    fn id(&self) -> String {
        match self {
            Self::File(path) => format!("file:{path}"),
            Self::Export(path, symbol) => format!("export:{path}:{symbol}"),
        }
    }
}

struct QueryGraphIndex {
    nodes: Vec<QueryNode>,
    edges: Vec<QueryEdge>,
    outgoing: Vec<Vec<usize>>,
    incoming: Vec<Vec<usize>>,
    node_by_key: BTreeMap<QueryNodeKey, usize>,
    node_by_id: BTreeMap<String, usize>,
}

impl QueryGraphIndex {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            outgoing: Vec::new(),
            incoming: Vec::new(),
            node_by_key: BTreeMap::new(),
            node_by_id: BTreeMap::new(),
        }
    }

    fn add_file(&mut self, path: String) -> usize {
        self.add_node(QueryNodeKey::File(path))
    }

    fn add_export(&mut self, path: String, symbol: String) -> usize {
        self.add_node(QueryNodeKey::Export(path, symbol))
    }

    fn add_node(&mut self, key: QueryNodeKey) -> usize {
        if let Some(index) = self.node_by_key.get(&key) {
            return *index;
        }

        let node = match &key {
            QueryNodeKey::File(path) => QueryNode {
                id: key.id(),
                kind: QueryNodeKind::File,
                path: path.clone(),
                symbol: None,
            },
            QueryNodeKey::Export(path, symbol) => QueryNode {
                id: key.id(),
                kind: QueryNodeKind::Export,
                path: path.clone(),
                symbol: Some(symbol.clone()),
            },
        };
        let index = self.nodes.len();
        self.node_by_id.insert(node.id.clone(), index);
        self.nodes.push(node);
        self.outgoing.push(Vec::new());
        self.incoming.push(Vec::new());
        self.node_by_key.insert(key, index);
        index
    }

    fn add_edge(&mut self, from: usize, to: usize, edge: QueryEdge) {
        let index = self.edges.len();
        self.edges.push(edge);
        self.outgoing[from].push(index);
        self.incoming[to].push(index);
    }

    fn node(&self, index: usize) -> QueryNode {
        self.nodes[index].clone()
    }

    fn edge(&self, index: usize) -> QueryEdge {
        self.edges[index].clone()
    }

    fn file_node(&self, path: &str) -> Option<usize> {
        self.node_by_key
            .get(&QueryNodeKey::File(path.to_string()))
            .copied()
    }

    fn export_node(&self, path: &str, symbol: &str) -> Option<usize> {
        self.node_by_key
            .get(&QueryNodeKey::Export(path.to_string(), symbol.to_string()))
            .copied()
    }
}

pub fn query_path(
    cwd: &Path,
    config: &CodescytheConfig,
    request: QueryRequest,
) -> Result<QueryResult> {
    let cwd = absolute_normalize_path(cwd)?;
    if !cwd.exists() {
        anyhow::bail!("analysis root does not exist: {}", cwd.display());
    }

    let project_files = discover_project_files(&cwd, config)?;
    let index_by_path = project_files
        .iter()
        .enumerate()
        .map(|(index, path)| (normalize_path(path), index))
        .collect::<HashMap<_, _>>();
    let module_resolver = ModuleResolver::new(&cwd, &project_files, config)?;
    let mut files = FileCache::new(&cwd, project_files)?;
    let all_indexes = (0..files.paths.len()).collect::<Vec<_>>();
    files.parse_many(&all_indexes)?;

    let (graph, unresolved) = build_query_graph(&mut files, &module_resolver)?;
    let from = parse_query_selector(&cwd, &request.from)?;
    let to = parse_query_selector(&cwd, &request.to)?;
    let source_indexes = resolve_selector(&graph, &files, &index_by_path, &from, "from")?;
    let target_indexes = resolve_selector(&graph, &files, &index_by_path, &to, "to")?;

    let source_nodes = source_indexes
        .iter()
        .copied()
        .map(|index| graph.node(index))
        .collect::<Vec<_>>();
    let target_nodes = target_indexes
        .iter()
        .copied()
        .map(|index| graph.node(index))
        .collect::<Vec<_>>();

    let target_set = target_indexes.iter().copied().collect::<HashSet<_>>();
    let paths = match request.kind {
        QueryKind::Somepath => shortest_paths(&graph, &source_indexes, &target_set, true),
        QueryKind::Somepaths => shortest_paths(&graph, &source_indexes, &target_set, false),
        QueryKind::Allpaths => Vec::new(),
    };
    let path_graph = match request.kind {
        QueryKind::Allpaths => Some(allpaths_graph(&graph, &source_indexes, &target_set)),
        QueryKind::Somepath | QueryKind::Somepaths => None,
    };

    Ok(QueryResult {
        kind: request.kind,
        from,
        to,
        source_nodes,
        target_nodes,
        paths,
        graph: path_graph,
        unresolved,
    })
}

fn build_query_graph(
    files: &mut FileCache,
    resolver: &ModuleResolver,
) -> Result<(QueryGraphIndex, Vec<QueryUnresolvedImport>)> {
    let mut graph = QueryGraphIndex::new();
    let mut file_nodes = Vec::with_capacity(files.paths.len());
    let mut unresolved = BTreeSet::<QueryUnresolvedImport>::new();

    for index in 0..files.paths.len() {
        let relative = files.relative(index);
        file_nodes.push(graph.add_file(relative.clone()));
        let file = files.get(index)?;
        for name in file.exports.keys() {
            let export = graph.add_export(relative.clone(), name.clone());
            graph.add_edge(
                export,
                file_nodes[index],
                QueryEdge {
                    from: graph.node(export).id,
                    to: graph.node(file_nodes[index]).id,
                    kind: QueryEdgeKind::ExportDefinition,
                    importer: None,
                    specifier: None,
                    imported: Some(name.clone()),
                },
            );
        }
    }

    for index in 0..files.paths.len() {
        let file = files.get(index)?.clone();
        let from = file_nodes[index];

        for import in &file.imports {
            match resolver.resolve(&file, &import.source)? {
                ImportResolution::Project(target) => {
                    if let Some(imported) = &import.imported
                        && let Some(to) = graph.export_node(&files.relative(target), imported)
                    {
                        graph.add_edge(
                            from,
                            to,
                            import_edge(
                                &graph,
                                from,
                                to,
                                QueryEdgeKind::NamedImport,
                                &file.relative,
                                &import.source,
                                Some(imported),
                            ),
                        );
                    }
                }
                ImportResolution::Unresolved => {
                    unresolved.insert(QueryUnresolvedImport {
                        importer: file.relative.clone(),
                        specifier: import.source.clone(),
                    });
                }
                ImportResolution::External => {}
            }
        }

        for source in &file.side_effect_imports {
            add_file_edge(
                &mut graph,
                &mut unresolved,
                resolver,
                &file_nodes,
                &file,
                from,
                source,
                QueryEdgeKind::SideEffectImport,
            )?;
        }

        for source in &file.dynamic_imports {
            match resolver.resolve(&file, source)? {
                ImportResolution::Project(target) => {
                    add_dependency_file_edge(
                        &mut graph,
                        from,
                        file_nodes[target],
                        QueryEdgeKind::DynamicImport,
                        &file.relative,
                        source,
                    );
                    add_all_export_edges(
                        &mut graph,
                        files,
                        from,
                        target,
                        QueryEdgeKind::DynamicImport,
                        &file.relative,
                        source,
                    )?;
                }
                ImportResolution::Unresolved => {
                    unresolved.insert(QueryUnresolvedImport {
                        importer: file.relative.clone(),
                        specifier: source.clone(),
                    });
                }
                ImportResolution::External => {}
            }
        }

        for pattern in &file.glob_imports {
            let Some(glob) = project_glob_from_import(&file.relative, pattern) else {
                continue;
            };
            for target in files.matching_relative_glob(&glob)? {
                add_dependency_file_edge(
                    &mut graph,
                    from,
                    file_nodes[target],
                    QueryEdgeKind::GlobImport,
                    &file.relative,
                    pattern,
                );
                add_all_export_edges(
                    &mut graph,
                    files,
                    from,
                    target,
                    QueryEdgeKind::GlobImport,
                    &file.relative,
                    pattern,
                )?;
            }
        }

        for (local, member) in &file.member_uses {
            if let Some(source) = file.namespace_imports.get(local) {
                add_namespace_member_edge(
                    &mut graph,
                    &mut unresolved,
                    files,
                    resolver,
                    &file,
                    from,
                    source,
                    member,
                )?;
            }

            if let Some(named) = file.named_imports.get(local)
                && let ImportResolution::Project(target) = resolver.resolve(&file, &named.source)?
            {
                let target_file = files.get(target)?.clone();
                if let Some(namespace_source) = target_file
                    .exports
                    .get(&named.imported)
                    .and_then(|export| export.namespace_source.clone())
                {
                    add_namespace_member_edge(
                        &mut graph,
                        &mut unresolved,
                        files,
                        resolver,
                        &target_file,
                        from,
                        &namespace_source,
                        member,
                    )?;
                }
            }
        }

        for (export_name, export) in &file.exports {
            if let Some(source) = &export.reexport_source {
                add_file_edge(
                    &mut graph,
                    &mut unresolved,
                    resolver,
                    &file_nodes,
                    &file,
                    from,
                    source,
                    QueryEdgeKind::ReExportSource,
                )?;
            }

            if let (Some(source), Some(name)) = (&export.reexport_source, &export.reexport_name)
                && let ImportResolution::Project(target) = resolver.resolve(&file, source)?
                && let (Some(from_export), Some(to_export)) = (
                    graph.export_node(&file.relative, export_name),
                    graph.export_node(&files.relative(target), name),
                )
            {
                graph.add_edge(
                    from_export,
                    to_export,
                    import_edge(
                        &graph,
                        from_export,
                        to_export,
                        QueryEdgeKind::ReExport,
                        &file.relative,
                        source,
                        Some(name),
                    ),
                );
            }

            if let Some(source) = &export.namespace_source {
                match resolver.resolve(&file, source)? {
                    ImportResolution::Project(target) => {
                        add_dependency_file_edge(
                            &mut graph,
                            from,
                            file_nodes[target],
                            QueryEdgeKind::ReExportSource,
                            &file.relative,
                            source,
                        );
                        if let Some(from_export) = graph.export_node(&file.relative, export_name) {
                            graph.add_edge(
                                from_export,
                                file_nodes[target],
                                import_edge(
                                    &graph,
                                    from_export,
                                    file_nodes[target],
                                    QueryEdgeKind::NamespaceExport,
                                    &file.relative,
                                    source,
                                    Some(export_name),
                                ),
                            );
                        }
                    }
                    ImportResolution::Unresolved => {
                        unresolved.insert(QueryUnresolvedImport {
                            importer: file.relative.clone(),
                            specifier: source.clone(),
                        });
                    }
                    ImportResolution::External => {}
                }
            }
        }

        for source in &file.reexport_all {
            add_file_edge(
                &mut graph,
                &mut unresolved,
                resolver,
                &file_nodes,
                &file,
                from,
                source,
                QueryEdgeKind::ReExportSource,
            )?;
        }
    }

    sort_graph_edges(&mut graph);
    Ok((graph, unresolved.into_iter().collect()))
}

fn add_file_edge(
    graph: &mut QueryGraphIndex,
    unresolved: &mut BTreeSet<QueryUnresolvedImport>,
    resolver: &ModuleResolver,
    file_nodes: &[usize],
    file: &FileData,
    from: usize,
    source: &str,
    kind: QueryEdgeKind,
) -> Result<()> {
    match resolver.resolve(file, source)? {
        ImportResolution::Project(target) => {
            add_dependency_file_edge(
                graph,
                from,
                file_nodes[target],
                kind,
                &file.relative,
                source,
            );
        }
        ImportResolution::Unresolved => {
            unresolved.insert(QueryUnresolvedImport {
                importer: file.relative.clone(),
                specifier: source.to_string(),
            });
        }
        ImportResolution::External => {}
    }
    Ok(())
}

fn add_dependency_file_edge(
    graph: &mut QueryGraphIndex,
    from: usize,
    to: usize,
    kind: QueryEdgeKind,
    importer: &str,
    specifier: &str,
) {
    graph.add_edge(
        from,
        to,
        import_edge(graph, from, to, kind, importer, specifier, None),
    );
}

fn add_all_export_edges(
    graph: &mut QueryGraphIndex,
    files: &mut FileCache,
    from: usize,
    target: usize,
    kind: QueryEdgeKind,
    importer: &str,
    specifier: &str,
) -> Result<()> {
    let target_relative = files.relative(target);
    let exports = files
        .get(target)?
        .exports
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for name in exports {
        if let Some(to) = graph.export_node(&target_relative, &name) {
            graph.add_edge(
                from,
                to,
                import_edge(graph, from, to, kind, importer, specifier, Some(&name)),
            );
        }
    }
    Ok(())
}

fn add_namespace_member_edge(
    graph: &mut QueryGraphIndex,
    unresolved: &mut BTreeSet<QueryUnresolvedImport>,
    files: &mut FileCache,
    resolver: &ModuleResolver,
    file: &FileData,
    from: usize,
    source: &str,
    member: &str,
) -> Result<()> {
    match resolver.resolve(file, source)? {
        ImportResolution::Project(target) => {
            if let Some(to) = graph.export_node(&files.relative(target), member) {
                graph.add_edge(
                    from,
                    to,
                    import_edge(
                        graph,
                        from,
                        to,
                        QueryEdgeKind::NamespaceMember,
                        &file.relative,
                        source,
                        Some(member),
                    ),
                );
            }
        }
        ImportResolution::Unresolved => {
            unresolved.insert(QueryUnresolvedImport {
                importer: file.relative.clone(),
                specifier: source.to_string(),
            });
        }
        ImportResolution::External => {}
    }
    Ok(())
}

fn import_edge(
    graph: &QueryGraphIndex,
    from: usize,
    to: usize,
    kind: QueryEdgeKind,
    importer: &str,
    specifier: &str,
    imported: Option<&str>,
) -> QueryEdge {
    QueryEdge {
        from: graph.nodes[from].id.clone(),
        to: graph.nodes[to].id.clone(),
        kind,
        importer: Some(importer.to_string()),
        specifier: Some(specifier.to_string()),
        imported: imported.map(str::to_string),
    }
}

fn sort_graph_edges(graph: &mut QueryGraphIndex) {
    for edges in &mut graph.outgoing {
        edges.sort_by_key(|edge| graph.edges[*edge].clone());
    }
    for edges in &mut graph.incoming {
        edges.sort_by_key(|edge| graph.edges[*edge].clone());
    }
}

fn parse_query_selector(cwd: &Path, raw: &str) -> Result<QuerySelector> {
    let (path, symbol) = raw
        .rsplit_once(':')
        .filter(|(path, symbol)| !path.is_empty() && !symbol.is_empty())
        .map_or((raw, None), |(path, symbol)| {
            (path, Some(symbol.to_string()))
        });
    let path = selector_path(cwd, path);
    let kind = if symbol.is_some() {
        QuerySelectorKind::Export
    } else if raw.ends_with('/') || cwd.join(&path).is_dir() {
        QuerySelectorKind::Directory
    } else {
        QuerySelectorKind::File
    };

    Ok(QuerySelector {
        raw: raw.to_string(),
        kind,
        path,
        symbol,
    })
}

fn selector_path(cwd: &Path, path: &str) -> String {
    let path = Path::new(path);
    let normalized = if path.is_absolute() {
        normalize_path(path)
            .strip_prefix(cwd)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| normalize_path(path))
    } else {
        normalize_path(path)
    };
    normalized.to_string_lossy().replace('\\', "/")
}

fn resolve_selector(
    graph: &QueryGraphIndex,
    files: &FileCache,
    index_by_path: &HashMap<PathBuf, usize>,
    selector: &QuerySelector,
    label: &str,
) -> Result<Vec<usize>> {
    let mut indexes = match selector.kind {
        QuerySelectorKind::File => graph
            .file_node(&selector.path)
            .map(|index| vec![index])
            .unwrap_or_default(),
        QuerySelectorKind::Directory => {
            let prefix = selector.path.trim_end_matches('/');
            graph
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(index, node)| {
                    (node.kind == QueryNodeKind::File
                        && (node.path == prefix || node.path.starts_with(&format!("{prefix}/"))))
                    .then_some(index)
                })
                .collect::<Vec<_>>()
        }
        QuerySelectorKind::Export => graph
            .export_node(
                &selector.path,
                selector
                    .symbol
                    .as_deref()
                    .expect("export selectors should include a symbol"),
            )
            .map(|index| vec![index])
            .unwrap_or_default(),
    };
    indexes.sort_unstable();
    indexes.dedup();

    if indexes.is_empty() {
        let known_file =
            index_by_path.contains_key(&normalize_path(&files.cwd.join(&selector.path)));
        match selector.kind {
            QuerySelectorKind::Export if known_file => {
                anyhow::bail!(
                    "{label} selector {} did not match an exported symbol",
                    selector.raw
                );
            }
            _ => anyhow::bail!(
                "{label} selector {} did not match any project files",
                selector.raw
            ),
        }
    }
    Ok(indexes)
}

fn shortest_paths(
    graph: &QueryGraphIndex,
    sources: &[usize],
    targets: &HashSet<usize>,
    stop_after_first: bool,
) -> Vec<QueryPath> {
    let mut queue = VecDeque::new();
    let mut seen = vec![false; graph.nodes.len()];
    let mut parent_edge = vec![None; graph.nodes.len()];
    let mut sources = sources.to_vec();
    sources.sort_unstable();
    for source in sources {
        seen[source] = true;
        queue.push_back(source);
    }

    let mut found = BTreeSet::<usize>::new();
    while let Some(node) = queue.pop_front() {
        if targets.contains(&node) && found.insert(node) && stop_after_first {
            break;
        }

        for edge in &graph.outgoing[node] {
            let Some(next) = graph.node_by_id.get(&graph.edges[*edge].to).copied() else {
                continue;
            };
            if !seen[next] {
                seen[next] = true;
                parent_edge[next] = Some(*edge);
                queue.push_back(next);
            }
        }
    }

    found
        .into_iter()
        .map(|target| reconstruct_path(graph, target, &parent_edge))
        .collect()
}

fn reconstruct_path(
    graph: &QueryGraphIndex,
    target: usize,
    parent_edge: &[Option<usize>],
) -> QueryPath {
    let mut node_indexes = vec![target];
    let mut edge_indexes = Vec::new();
    let mut current = target;
    while let Some(edge) = parent_edge[current] {
        edge_indexes.push(edge);
        let previous = graph
            .node_by_id
            .get(&graph.edges[edge].from)
            .copied()
            .expect("edge endpoint should reference a node");
        node_indexes.push(previous);
        current = previous;
    }
    node_indexes.reverse();
    edge_indexes.reverse();
    QueryPath {
        nodes: node_indexes
            .into_iter()
            .map(|index| graph.node(index))
            .collect(),
        edges: edge_indexes
            .into_iter()
            .map(|index| graph.edge(index))
            .collect(),
    }
}

fn allpaths_graph(
    graph: &QueryGraphIndex,
    sources: &[usize],
    targets: &HashSet<usize>,
) -> QueryGraph {
    let reachable_from_sources = reachable(graph, sources, Direction::Forward);
    let targets = targets.iter().copied().collect::<Vec<_>>();
    let can_reach_targets = reachable(graph, &targets, Direction::Reverse);
    let path_nodes = reachable_from_sources
        .intersection(&can_reach_targets)
        .copied()
        .collect::<BTreeSet<_>>();

    let nodes = path_nodes
        .iter()
        .copied()
        .map(|index| graph.node(index))
        .collect::<Vec<_>>();
    let edges = graph
        .edges
        .iter()
        .filter(|edge| {
            let Some(from) = graph.node_by_id.get(&edge.from) else {
                return false;
            };
            let Some(to) = graph.node_by_id.get(&edge.to) else {
                return false;
            };
            path_nodes.contains(from) && path_nodes.contains(to)
        })
        .cloned()
        .collect::<Vec<_>>();

    QueryGraph { nodes, edges }
}

enum Direction {
    Forward,
    Reverse,
}

fn reachable(graph: &QueryGraphIndex, starts: &[usize], direction: Direction) -> BTreeSet<usize> {
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::new();
    for start in starts {
        if seen.insert(*start) {
            queue.push_back(*start);
        }
    }

    while let Some(node) = queue.pop_front() {
        let edges = match direction {
            Direction::Forward => &graph.outgoing[node],
            Direction::Reverse => &graph.incoming[node],
        };
        for edge in edges {
            let endpoint = match direction {
                Direction::Forward => &graph.edges[*edge].to,
                Direction::Reverse => &graph.edges[*edge].from,
            };
            let Some(next) = graph.node_by_id.get(endpoint).copied() else {
                continue;
            };
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    seen
}
