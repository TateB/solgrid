use crate::{GraphFormatArg, GraphKindArg};
use solgrid_config::find_workspace_root;
use solgrid_project::{
    GraphDocument, GraphEdgeKind, GraphKind, GraphLensSpec, GraphNode, GraphNodeKind, ProjectIndex,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn run(kind: &GraphKindArg, path: &Path, symbol: Option<&str>, format: &GraphFormatArg) -> i32 {
    let path = match normalize_graph_path(path) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };

    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("Failed to read {}: {error}", path.display());
            return 1;
        }
    };

    let graph = match build_graph_document(kind, &path, &source, symbol) {
        Ok(graph) => graph,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };

    match format_graph_document(&graph, format) {
        Ok(output) => {
            println!("{output}");
            0
        }
        Err(error) => {
            eprintln!("Failed to render graph output: {error}");
            1
        }
    }
}

fn normalize_graph_path(path: &Path) -> Result<PathBuf, String> {
    let path = fs::canonicalize(path)
        .map_err(|error| format!("Failed to resolve {}: {error}", path.display()))?;
    if !path.is_file() {
        return Err(format!(
            "Graph target must be a Solidity file, got {}",
            path.display()
        ));
    }
    if path.extension().is_none_or(|ext| ext != "sol") {
        return Err(format!(
            "Graph target must be a .sol file, got {}",
            path.display()
        ));
    }
    Ok(path)
}

fn build_graph_document(
    kind: &GraphKindArg,
    path: &Path,
    source: &str,
    symbol: Option<&str>,
) -> Result<GraphDocument, String> {
    let graph_kind = graph_kind(kind);
    let workspace_root = graph_workspace_root(path);
    let index = ProjectIndex::build(&workspace_root);
    let get_source = |path: &Path| fs::read_to_string(path).ok();

    match graph_kind {
        GraphKind::Imports => {
            if symbol.is_some() {
                return Err("--symbol is only valid for inheritance or control-flow graphs".into());
            }
            index
                .imports_graph(path, source, &get_source)
                .ok_or_else(|| format!("No imports graph could be built for {}", path.display()))
        }
        GraphKind::Inheritance => {
            let target = select_graph_lens(&index, path, source, graph_kind, symbol)?;
            index
                .inheritance_graph(
                    path,
                    source,
                    target
                        .symbol_name
                        .as_deref()
                        .expect("inheritance lens must have a symbol"),
                    &get_source,
                )
                .ok_or_else(|| {
                    format!("No inheritance graph could be built for {}", path.display())
                })
        }
        GraphKind::LinearizedInheritance => {
            let target = select_graph_lens(&index, path, source, graph_kind, symbol)?;
            index
                .linearized_inheritance_graph(
                    path,
                    source,
                    target
                        .symbol_name
                        .as_deref()
                        .expect("linearized inheritance lens must have a symbol"),
                    &get_source,
                )
                .ok_or_else(|| {
                    format!(
                        "No linearized inheritance graph could be built for {}",
                        path.display()
                    )
                })
        }
        GraphKind::ControlFlow => {
            let target = select_graph_lens(&index, path, source, graph_kind, symbol)?;
            index
                .control_flow_graph(
                    path,
                    source,
                    target
                        .target_offset
                        .expect("control-flow lens must have a target offset"),
                )
                .ok_or_else(|| {
                    format!(
                        "No control-flow graph could be built for {}",
                        path.display()
                    )
                })
        }
    }
}

fn graph_workspace_root(path: &Path) -> PathBuf {
    let start = path.parent().unwrap_or(path);
    find_workspace_root(start).unwrap_or_else(|| start.to_path_buf())
}

fn select_graph_lens(
    index: &ProjectIndex,
    path: &Path,
    source: &str,
    kind: GraphKind,
    symbol: Option<&str>,
) -> Result<GraphLensSpec, String> {
    let mut candidates = index
        .graph_lenses(path, source)
        .into_iter()
        .filter(|lens| lens.kind == kind)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.symbol_name.cmp(&right.symbol_name));

    if candidates.is_empty() {
        return Err(match kind {
            GraphKind::Inheritance => {
                format!(
                    "No inheritance graph targets were found in {}",
                    path.display()
                )
            }
            GraphKind::LinearizedInheritance => format!(
                "No linearized inheritance graph targets were found in {}",
                path.display()
            ),
            GraphKind::ControlFlow => {
                format!(
                    "No control-flow graph targets were found in {}",
                    path.display()
                )
            }
            GraphKind::Imports => unreachable!("imports graphs do not use lens selection"),
        });
    }

    if let Some(symbol) = symbol {
        if let Some(target) = candidates
            .iter()
            .find(|lens| lens.symbol_name.as_deref() == Some(symbol))
            .cloned()
        {
            return Ok(target);
        }

        let available = available_graph_symbols(&candidates);
        return Err(format!(
            "No {} graph target named `{symbol}` in {}. Available targets: {}",
            graph_kind_label(kind),
            path.display(),
            available.join(", ")
        ));
    }

    if candidates.len() == 1 {
        return Ok(candidates.remove(0));
    }

    let available = available_graph_symbols(&candidates);
    Err(format!(
        "{} graph for {} requires --symbol. Available targets: {}",
        graph_kind_label(kind),
        path.display(),
        available.join(", ")
    ))
}

fn available_graph_symbols(candidates: &[GraphLensSpec]) -> Vec<String> {
    candidates
        .iter()
        .filter_map(|lens| lens.symbol_name.clone())
        .collect()
}

fn format_graph_document(
    graph: &GraphDocument,
    format: &GraphFormatArg,
) -> Result<String, serde_json::Error> {
    match format {
        GraphFormatArg::Json => serde_json::to_string_pretty(graph),
        GraphFormatArg::Mermaid => Ok(render_graph_mermaid(graph)),
        GraphFormatArg::Dot => Ok(render_graph_dot(graph)),
    }
}

fn graph_kind(kind: &GraphKindArg) -> GraphKind {
    match kind {
        GraphKindArg::Imports => GraphKind::Imports,
        GraphKindArg::Inheritance => GraphKind::Inheritance,
        GraphKindArg::LinearizedInheritance => GraphKind::LinearizedInheritance,
        GraphKindArg::ControlFlow => GraphKind::ControlFlow,
    }
}

fn graph_kind_label(kind: GraphKind) -> &'static str {
    match kind {
        GraphKind::Imports => "imports",
        GraphKind::Inheritance => "inheritance",
        GraphKind::LinearizedInheritance => "linearized inheritance",
        GraphKind::ControlFlow => "control-flow",
    }
}

fn render_graph_mermaid(graph: &GraphDocument) -> String {
    let mut lines = vec![format!(
        "flowchart {}",
        if graph.kind == GraphKind::ControlFlow {
            "TD"
        } else {
            "LR"
        }
    )];

    let mut ids = BTreeMap::new();
    for (index, node) in graph.nodes.iter().enumerate() {
        let id = format!("n{index}");
        ids.insert(node.id.clone(), id.clone());
        lines.push(format!(
            "    {id}[\"{}\"]",
            escape_mermaid_label(&node.label)
        ));
    }

    for edge in &graph.edges {
        let Some(from) = ids.get(&edge.from) else {
            continue;
        };
        let Some(to) = ids.get(&edge.to) else {
            continue;
        };
        let label = edge
            .label
            .as_ref()
            .map(|label| format!("|{}|", escape_mermaid_label(label)))
            .unwrap_or_default();
        lines.push(format!(
            "    {from} {}{label} {to}",
            mermaid_connector(edge.kind)
        ));
    }

    if let Some(focus) = &graph.focus_node_id {
        if let Some(node_id) = ids.get(focus) {
            lines.push(format!(
                "    style {node_id} fill:#d9f2e6,stroke:#0b6e4f,stroke-width:2px"
            ));
        }
    }

    for (class_name, node_ids) in mermaid_node_classes(&graph.nodes, &ids) {
        if !node_ids.is_empty() {
            lines.push(format!("    class {} {class_name}", node_ids.join(",")));
        }
    }

    for class_def in mermaid_class_definitions() {
        lines.push(format!("    {class_def}"));
    }

    lines.join("\n")
}

fn render_graph_dot(graph: &GraphDocument) -> String {
    let mut lines = vec![
        "digraph solgrid {".to_string(),
        format!(
            "    rankdir={};",
            if graph.kind == GraphKind::ControlFlow {
                "TB"
            } else {
                "LR"
            }
        ),
        "    graph [fontname=\"Helvetica\"];".to_string(),
        "    node [fontname=\"Helvetica\", shape=box, style=\"rounded,filled\"];".to_string(),
        "    edge [fontname=\"Helvetica\"];".to_string(),
    ];

    let focus = graph.focus_node_id.as_deref();
    for (index, node) in graph.nodes.iter().enumerate() {
        lines.push(format!(
            "    n{index} [{}];",
            dot_node_attributes(node, focus == Some(node.id.as_str())).join(", ")
        ));
    }

    let ids = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), format!("n{index}")))
        .collect::<BTreeMap<_, _>>();

    for edge in &graph.edges {
        let Some(from) = ids.get(edge.from.as_str()) else {
            continue;
        };
        let Some(to) = ids.get(edge.to.as_str()) else {
            continue;
        };
        let attrs = dot_edge_attributes(edge);
        if attrs.is_empty() {
            lines.push(format!("    {from} -> {to};"));
        } else {
            lines.push(format!("    {from} -> {to} [{}];", attrs.join(", ")));
        }
    }

    lines.push("}".to_string());
    lines.join("\n")
}

fn mermaid_connector(kind: Option<GraphEdgeKind>) -> &'static str {
    match kind {
        Some(GraphEdgeKind::LoopBack | GraphEdgeKind::Break | GraphEdgeKind::Continue) => "-.->",
        Some(GraphEdgeKind::Return | GraphEdgeKind::Revert) => "==>",
        _ => "-->",
    }
}

fn mermaid_node_classes(
    nodes: &[GraphNode],
    ids: &BTreeMap<String, String>,
) -> Vec<(String, Vec<String>)> {
    let mut buckets = BTreeMap::<String, Vec<String>>::new();
    for node in nodes {
        let Some(id) = ids.get(&node.id) else {
            continue;
        };
        let Some(class_name) = mermaid_node_class(node.kind) else {
            continue;
        };
        buckets
            .entry(class_name.to_string())
            .or_default()
            .push(id.clone());
    }
    buckets.into_iter().collect()
}

fn mermaid_node_class(kind: Option<GraphNodeKind>) -> Option<&'static str> {
    match kind {
        Some(GraphNodeKind::File) => Some("file"),
        Some(GraphNodeKind::Contract) => Some("contract"),
        Some(GraphNodeKind::Entry) => Some("entry"),
        Some(GraphNodeKind::Exit) => Some("exit"),
        Some(GraphNodeKind::Modifier) => Some("modifier"),
        Some(GraphNodeKind::Declaration | GraphNodeKind::Assignment) => Some("state"),
        Some(GraphNodeKind::Call | GraphNodeKind::Emit) => Some("call"),
        Some(GraphNodeKind::Branch) => Some("branch"),
        Some(GraphNodeKind::Loop | GraphNodeKind::LoopNext) => Some("loop"),
        Some(
            GraphNodeKind::TerminalReturn
            | GraphNodeKind::TerminalRevert
            | GraphNodeKind::ControlTransfer,
        ) => Some("terminal"),
        Some(GraphNodeKind::Assembly) => Some("opaque"),
        Some(GraphNodeKind::Try | GraphNodeKind::Catch | GraphNodeKind::Block) => {
            Some("structural")
        }
        Some(GraphNodeKind::Statement) | None => None,
    }
}

fn mermaid_class_definitions() -> [&'static str; 12] {
    [
        "classDef file fill:#eef2ff,stroke:#4f46e5,stroke-width:1.5px",
        "classDef contract fill:#ecfeff,stroke:#0f766e,stroke-width:1.5px",
        "classDef entry fill:#dcfce7,stroke:#166534,stroke-width:1.5px",
        "classDef exit fill:#fee2e2,stroke:#b91c1c,stroke-width:1.5px",
        "classDef modifier fill:#fff7ed,stroke:#c2410c,stroke-width:1.5px",
        "classDef state fill:#eff6ff,stroke:#1d4ed8,stroke-width:1.5px",
        "classDef call fill:#ecfeff,stroke:#0f766e,stroke-width:1.5px",
        "classDef branch fill:#fef9c3,stroke:#a16207,stroke-width:1.5px",
        "classDef loop fill:#dbeafe,stroke:#1d4ed8,stroke-width:1.5px",
        "classDef terminal fill:#fee2e2,stroke:#b91c1c,stroke-width:1.5px",
        "classDef opaque fill:#e5e7eb,stroke:#4b5563,stroke-width:1.5px",
        "classDef structural fill:#f3f4f6,stroke:#374151,stroke-width:1.5px",
    ]
}

fn escape_mermaid_label(label: &str) -> String {
    label.replace('\\', "\\\\").replace('"', "\\\"")
}

fn dot_node_attributes(node: &GraphNode, focused: bool) -> Vec<String> {
    let mut attrs = vec![format!("label=\"{}\"", escape_dot_label(&node.label))];
    let (fillcolor, color, shape) = dot_node_style(node.kind);
    attrs.push(format!("fillcolor=\"{fillcolor}\""));
    attrs.push(format!("color=\"{color}\""));
    attrs.push(format!("shape=\"{shape}\""));
    if !node.detail.is_empty() {
        attrs.push(format!("tooltip=\"{}\"", escape_dot_label(&node.detail)));
    }
    if let Some(uri) = &node.uri {
        attrs.push(format!("URL=\"{}\"", escape_dot_label(uri)));
    }
    if focused {
        attrs.push("penwidth=2".to_string());
    }
    attrs
}

fn dot_edge_attributes(edge: &solgrid_project::GraphEdge) -> Vec<String> {
    let mut attrs = Vec::new();
    if let Some(label) = &edge.label {
        attrs.push(format!("label=\"{}\"", escape_dot_label(label)));
    }
    let (style, color, penwidth) = dot_edge_style(edge.kind);
    attrs.push(format!("style=\"{style}\""));
    attrs.push(format!("color=\"{color}\""));
    attrs.push(format!("penwidth={penwidth}"));
    attrs
}

fn dot_node_style(kind: Option<GraphNodeKind>) -> (&'static str, &'static str, &'static str) {
    match kind {
        Some(GraphNodeKind::File) => ("#eef2ff", "#4f46e5", "box"),
        Some(GraphNodeKind::Contract) => ("#ecfeff", "#0f766e", "box"),
        Some(GraphNodeKind::Entry) => ("#dcfce7", "#166534", "oval"),
        Some(GraphNodeKind::Exit) => ("#fee2e2", "#b91c1c", "oval"),
        Some(GraphNodeKind::Modifier) => ("#fff7ed", "#c2410c", "box"),
        Some(GraphNodeKind::Declaration | GraphNodeKind::Assignment) => {
            ("#eff6ff", "#1d4ed8", "box")
        }
        Some(GraphNodeKind::Call | GraphNodeKind::Emit) => ("#ecfeff", "#0f766e", "box"),
        Some(GraphNodeKind::Branch) => ("#fef9c3", "#a16207", "diamond"),
        Some(GraphNodeKind::Loop | GraphNodeKind::LoopNext) => ("#dbeafe", "#1d4ed8", "hexagon"),
        Some(
            GraphNodeKind::TerminalReturn
            | GraphNodeKind::TerminalRevert
            | GraphNodeKind::ControlTransfer,
        ) => ("#fee2e2", "#b91c1c", "box"),
        Some(GraphNodeKind::Assembly) => ("#e5e7eb", "#4b5563", "box"),
        Some(GraphNodeKind::Try | GraphNodeKind::Catch | GraphNodeKind::Block) => {
            ("#f3f4f6", "#374151", "box")
        }
        Some(GraphNodeKind::Statement) | None => ("#ffffff", "#6b7280", "box"),
    }
}

fn dot_edge_style(kind: Option<GraphEdgeKind>) -> (&'static str, &'static str, u8) {
    match kind {
        Some(GraphEdgeKind::LoopBack | GraphEdgeKind::Break | GraphEdgeKind::Continue) => {
            ("dashed", "#2563eb", 1)
        }
        Some(GraphEdgeKind::Return | GraphEdgeKind::Revert) => ("bold", "#b91c1c", 2),
        Some(GraphEdgeKind::Imports) => ("solid", "#4f46e5", 1),
        Some(GraphEdgeKind::Inherits | GraphEdgeKind::Precedes) => ("solid", "#0f766e", 1),
        Some(GraphEdgeKind::BranchTrue) => ("solid", "#15803d", 1),
        Some(GraphEdgeKind::BranchFalse) => ("solid", "#b45309", 1),
        Some(GraphEdgeKind::Normal) | None => ("solid", "#374151", 1),
    }
}

fn escape_dot_label(label: &str) -> String {
    label
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use solgrid_project::GraphEdge;
    use std::fs;

    #[test]
    fn test_build_graph_document_requires_symbol_for_ambiguous_control_flow() {
        let root = temp_workspace("graph_ambiguous");
        let main = root.join("Main.sol");
        fs::write(
            &main,
            r#"pragma solidity ^0.8.0;
contract Main {
    function deposit() public {}
    function withdraw() public {}
}
"#,
        )
        .unwrap();
        fs::write(root.join("solgrid.toml"), "").unwrap();

        let source = fs::read_to_string(&main).unwrap();
        let error = build_graph_document(&GraphKindArg::ControlFlow, &main, &source, None)
            .expect_err("control-flow graph should require a symbol when multiple functions exist");
        assert!(error.contains("requires --symbol"));
        assert!(error.contains("Main.deposit"));
        assert!(error.contains("Main.withdraw"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_build_graph_document_exports_inheritance_graph() {
        let root = temp_workspace("graph_inheritance");
        let main = root.join("Main.sol");
        fs::write(
            &main,
            r#"pragma solidity ^0.8.0;
contract Base {}
contract Main is Base {}
"#,
        )
        .unwrap();
        fs::write(root.join("solgrid.toml"), "").unwrap();

        let source = fs::read_to_string(&main).unwrap();
        let graph = build_graph_document(&GraphKindArg::Inheritance, &main, &source, Some("Main"))
            .expect("inheritance graph");
        assert_eq!(graph.kind, GraphKind::Inheritance);
        assert!(graph.nodes.iter().any(|node| node.label == "Main"));
        assert!(graph.nodes.iter().any(|node| node.label == "Base"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_render_graph_mermaid_uses_semantic_connectors_and_classes() {
        let graph = GraphDocument {
            kind: GraphKind::ControlFlow,
            title: "CFG".to_string(),
            nodes: vec![
                GraphNode {
                    id: "entry".to_string(),
                    label: "Entry".to_string(),
                    detail: String::new(),
                    kind: Some(GraphNodeKind::Entry),
                    uri: None,
                },
                GraphNode {
                    id: "return".to_string(),
                    label: "return".to_string(),
                    detail: String::new(),
                    kind: Some(GraphNodeKind::TerminalReturn),
                    uri: None,
                },
            ],
            edges: vec![GraphEdge {
                from: "entry".to_string(),
                to: "return".to_string(),
                label: Some("return".to_string()),
                kind: Some(GraphEdgeKind::Return),
            }],
            focus_node_id: Some("entry".to_string()),
        };

        let rendered = render_graph_mermaid(&graph);
        assert!(rendered.contains("flowchart TD"));
        assert!(rendered.contains("==>|return|"));
        assert!(rendered.contains("class n0 entry"));
        assert!(rendered.contains("class n1 terminal"));
        assert!(rendered.contains("style n0"));
    }

    #[test]
    fn test_render_graph_dot_uses_semantic_shapes_and_styles() {
        let graph = GraphDocument {
            kind: GraphKind::ControlFlow,
            title: "CFG".to_string(),
            nodes: vec![
                GraphNode {
                    id: "entry".to_string(),
                    label: "Entry".to_string(),
                    detail: "Start".to_string(),
                    kind: Some(GraphNodeKind::Entry),
                    uri: Some("file:///tmp/Main.sol".to_string()),
                },
                GraphNode {
                    id: "branch".to_string(),
                    label: "if ready".to_string(),
                    detail: String::new(),
                    kind: Some(GraphNodeKind::Branch),
                    uri: None,
                },
                GraphNode {
                    id: "return".to_string(),
                    label: "return".to_string(),
                    detail: String::new(),
                    kind: Some(GraphNodeKind::TerminalReturn),
                    uri: None,
                },
            ],
            edges: vec![
                GraphEdge {
                    from: "entry".to_string(),
                    to: "branch".to_string(),
                    label: Some("next".to_string()),
                    kind: Some(GraphEdgeKind::Normal),
                },
                GraphEdge {
                    from: "branch".to_string(),
                    to: "return".to_string(),
                    label: Some("return".to_string()),
                    kind: Some(GraphEdgeKind::Return),
                },
            ],
            focus_node_id: Some("entry".to_string()),
        };

        let rendered = render_graph_dot(&graph);
        assert!(rendered.contains("digraph solgrid {"));
        assert!(rendered.contains("rankdir=TB;"));
        assert!(rendered.contains("shape=\"oval\""));
        assert!(rendered.contains("shape=\"diamond\""));
        assert!(rendered.contains("penwidth=2"));
        assert!(rendered.contains("URL=\"file:///tmp/Main.sol\""));
        assert!(rendered.contains("label=\"return\""));
        assert!(rendered.contains("style=\"bold\""));
        assert!(rendered.contains("color=\"#b91c1c\""));
    }

    fn temp_workspace(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "solgrid_{}_{}_{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
