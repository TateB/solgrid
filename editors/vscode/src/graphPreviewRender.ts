export type GraphKind =
  | "imports"
  | "inheritance"
  | "linearized-inheritance"
  | "control-flow";

export type GraphNodeKind =
  | "file"
  | "contract"
  | "entry"
  | "exit"
  | "modifier"
  | "declaration"
  | "assignment"
  | "call"
  | "emit"
  | "branch"
  | "loop"
  | "loop-next"
  | "terminal-return"
  | "terminal-revert"
  | "control-transfer"
  | "assembly"
  | "try"
  | "catch"
  | "block"
  | "statement";

export type GraphEdgeKind =
  | "imports"
  | "inherits"
  | "precedes"
  | "normal"
  | "branch-true"
  | "branch-false"
  | "loop-back"
  | "return"
  | "revert"
  | "break"
  | "continue";

export interface GraphNodeLike {
  id: string;
  label: string;
  detail: string;
  kind?: GraphNodeKind;
  uri?: string;
}

export interface GraphEdgeLike {
  from: string;
  to: string;
  label?: string;
  kind?: GraphEdgeKind;
}

export interface GraphDocumentLike {
  kind: GraphKind;
  title: string;
  nodes: GraphNodeLike[];
  edges: GraphEdgeLike[];
  focusNodeId?: string;
}

export function renderGraphMarkdown(graph: GraphDocumentLike): string {
  const lines = [`# ${graph.title}`, "", graphSummary(graph), ""];
  if (graph.nodes.length === 0) {
    lines.push("No graph nodes were produced.");
    return lines.join("\n");
  }

  lines.push("```mermaid");
  lines.push(`flowchart ${graph.kind === "control-flow" ? "TD" : "LR"}`);

  const ids = new Map<string, string>();
  graph.nodes.forEach((node, index) => {
    ids.set(node.id, `n${index}`);
    lines.push(`    n${index}["${escapeMermaidLabel(node.label)}"]`);
  });

  for (const edge of graph.edges) {
    const from = ids.get(edge.from);
    const to = ids.get(edge.to);
    if (!from || !to) {
      continue;
    }
    const label = edge.label ? `|${escapeMermaidLabel(edge.label)}|` : "";
    lines.push(`    ${from} ${edgeConnector(edge.kind)}${label} ${to}`);
  }

  if (graph.focusNodeId) {
    const focusId = ids.get(graph.focusNodeId);
    if (focusId) {
      lines.push(`    style ${focusId} fill:#d9f2e6,stroke:#0b6e4f,stroke-width:2px`);
    }
  }

  for (const [className, nodeIds] of mermaidNodeClasses(graph.nodes, ids)) {
    if (nodeIds.length > 0) {
      lines.push(`    class ${nodeIds.join(",")} ${className}`);
    }
  }

  for (const definition of mermaidClassDefinitions()) {
    lines.push(`    ${definition}`);
  }

  lines.push("```", "", "## Nodes", "");
  for (const node of graph.nodes) {
    const location = node.uri ? ` ([source](${node.uri}))` : "";
    const kind = node.kind ? ` \`${node.kind}\`` : "";
    lines.push(`- **${node.label}**${kind}: ${node.detail}${location}`);
  }

  if (graph.kind === "linearized-inheritance") {
    lines.push("", "## Linearization", "");
    graph.nodes.forEach((node, index) => {
      lines.push(`${index + 1}. \`${node.label}\``);
    });
  }

  return lines.join("\n");
}

function graphSummary(graph: GraphDocumentLike): string {
  const edgeLabel = graph.edges.length === 1 ? "edge" : "edges";
  const nodeLabel = graph.nodes.length === 1 ? "node" : "nodes";
  if (graph.kind === "linearized-inheritance" && graph.nodes.length > 0) {
    return `${graph.nodes.length} ${nodeLabel}, ${graph.edges.length} ${edgeLabel}. Order: ${graph.nodes
      .map((node) => node.label)
      .join(" -> ")}`;
  }
  if (graph.kind === "control-flow") {
    const semanticCounts = summarizeControlFlowKinds(graph.nodes);
    const semanticSummary = [
      semanticCounts.branch > 0
        ? `${semanticCounts.branch} ${pluralize(semanticCounts.branch, "branch node")}`
        : null,
      semanticCounts.loop > 0
        ? `${semanticCounts.loop} ${pluralize(semanticCounts.loop, "loop node")}`
        : null,
      semanticCounts.call > 0
        ? `${semanticCounts.call} ${pluralize(semanticCounts.call, "call/emission node")}`
        : null,
      semanticCounts.terminal > 0
        ? `${semanticCounts.terminal} ${pluralize(semanticCounts.terminal, "terminal node")}`
        : null,
      semanticCounts.modifier > 0
        ? `${semanticCounts.modifier} ${pluralize(semanticCounts.modifier, "modifier node")}`
        : null,
      semanticCounts.assembly > 0
        ? `${semanticCounts.assembly} ${pluralize(semanticCounts.assembly, "assembly node")}`
        : null,
    ]
      .filter((value): value is string => value !== null)
      .join(", ");
    return `${graph.nodes.length} ${nodeLabel}, ${graph.edges.length} ${edgeLabel}. Function-level CFG with entry and exit nodes${
      semanticSummary ? `, including ${semanticSummary}` : ""
    }.`;
  }
  return `${graph.nodes.length} ${nodeLabel}, ${graph.edges.length} ${edgeLabel}`;
}

function edgeConnector(kind?: GraphEdgeKind): string {
  switch (kind) {
    case "loop-back":
    case "break":
    case "continue":
      return "-.->";
    case "return":
    case "revert":
      return "==>";
    default:
      return "-->";
  }
}

function mermaidNodeClasses(
  nodes: GraphNodeLike[],
  ids: Map<string, string>
): Array<[string, string[]]> {
  const buckets = new Map<string, string[]>();
  for (const node of nodes) {
    const id = ids.get(node.id);
    const className = mermaidNodeClass(node.kind);
    if (!id || !className) {
      continue;
    }
    const bucket = buckets.get(className) ?? [];
    bucket.push(id);
    buckets.set(className, bucket);
  }
  return Array.from(buckets.entries());
}

function mermaidNodeClass(kind?: GraphNodeKind): string | undefined {
  switch (kind) {
    case "file":
      return "file";
    case "contract":
      return "contract";
    case "entry":
      return "entry";
    case "exit":
      return "exit";
    case "modifier":
      return "modifier";
    case "declaration":
    case "assignment":
      return "state";
    case "call":
    case "emit":
      return "call";
    case "branch":
      return "branch";
    case "loop":
    case "loop-next":
      return "loop";
    case "terminal-return":
    case "terminal-revert":
    case "control-transfer":
      return "terminal";
    case "assembly":
      return "opaque";
    case "try":
    case "catch":
    case "block":
      return "structural";
    default:
      return undefined;
  }
}

function mermaidClassDefinitions(): string[] {
  return [
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
  ];
}

function summarizeControlFlowKinds(nodes: GraphNodeLike[]): {
  assembly: number;
  branch: number;
  call: number;
  loop: number;
  modifier: number;
  terminal: number;
} {
  return nodes.reduce(
    (counts, node) => {
      switch (node.kind) {
        case "assembly":
          counts.assembly += 1;
          break;
        case "branch":
          counts.branch += 1;
          break;
        case "call":
        case "emit":
          counts.call += 1;
          break;
        case "loop":
        case "loop-next":
          counts.loop += 1;
          break;
        case "modifier":
          counts.modifier += 1;
          break;
        case "terminal-return":
        case "terminal-revert":
        case "control-transfer":
          counts.terminal += 1;
          break;
      }
      return counts;
    },
    { assembly: 0, branch: 0, call: 0, loop: 0, modifier: 0, terminal: 0 }
  );
}

function pluralize(count: number, singular: string): string {
  return count === 1 ? singular : `${singular}s`;
}

function escapeMermaidLabel(value: string): string {
  return value.replace(/"/g, "'").replace(/\n/g, "<br/>");
}
