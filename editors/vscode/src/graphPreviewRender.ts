import { basename } from "node:path";
import { fileURLToPath } from "node:url";

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

export interface GraphPreviewSnapshot {
  kind: GraphKind;
  title: string;
  summary: string;
  focusLabel?: string;
  nodeLabels: string[];
  edgeCount: number;
}

interface LayoutNode extends GraphNodeLike {
  height: number;
  width: number;
  x: number;
  y: number;
}

interface LayoutEdge extends GraphEdgeLike {
  labelX: number;
  labelY: number;
  path: string;
}

interface LayoutGraph {
  direction: "LR" | "TD";
  edges: LayoutEdge[];
  height: number;
  nodes: LayoutNode[];
  width: number;
}

interface RenderOptions {
  cspSource: string;
  nonce: string;
}

const CARD_GAP = 24;
const H_PADDING = 40;
const NODE_RADIUS = 18;
const NODE_WIDTH_MAX = 320;
const NODE_WIDTH_MIN = 180;
const V_PADDING = 40;

export function buildGraphPreviewSnapshot(
  graph: GraphDocumentLike
): GraphPreviewSnapshot {
  const focusLabel = graph.focusNodeId
    ? graph.nodes.find((node) => node.id === graph.focusNodeId)?.label
    : undefined;

  return {
    edgeCount: graph.edges.length,
    focusLabel,
    kind: graph.kind,
    nodeLabels: graph.nodes.map((node) => node.label),
    summary: graphSummary(graph),
    title: graph.title,
  };
}

export function renderGraphWebviewHtml(
  graph: GraphDocumentLike,
  options: RenderOptions
): string {
  const layout = layoutGraph(graph);
  const snapshot = buildGraphPreviewSnapshot(graph);
  const linearizedList =
    graph.kind === "linearized-inheritance"
      ? `<section class="panel-section">
          <h2>Linearization</h2>
          <ol class="lineage-list">
            ${graph.nodes
              .map(
                (node, index) =>
                  `<li><span class="lineage-index">${index + 1}</span><span>${escapeHtml(
                    node.label
                  )}</span></li>`
              )
              .join("")}
          </ol>
        </section>`
      : "";

  return `<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta
      http-equiv="Content-Security-Policy"
      content="default-src 'none'; img-src ${options.cspSource} data:; style-src 'nonce-${options.nonce}'; script-src 'nonce-${options.nonce}';"
    />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>${escapeHtml(graph.title)}</title>
    <style nonce="${options.nonce}">
      :root {
        color-scheme: light dark;
        --panel-bg: color-mix(in srgb, var(--vscode-editor-background) 88%, transparent);
        --panel-border: color-mix(in srgb, var(--vscode-panel-border) 72%, transparent);
        --panel-muted: color-mix(in srgb, var(--vscode-descriptionForeground) 82%, transparent);
        --panel-strong: var(--vscode-foreground);
        --canvas-bg:
          radial-gradient(circle at top left, color-mix(in srgb, var(--vscode-textLink-foreground) 12%, transparent), transparent 34%),
          radial-gradient(circle at bottom right, color-mix(in srgb, var(--vscode-terminal-ansiGreen) 10%, transparent), transparent 40%),
          linear-gradient(180deg, color-mix(in srgb, var(--vscode-editor-background) 96%, transparent), color-mix(in srgb, var(--vscode-sideBar-background) 98%, transparent));
        --accent: color-mix(in srgb, var(--vscode-textLink-foreground) 86%, white 14%);
        --accent-soft: color-mix(in srgb, var(--vscode-textLink-foreground) 16%, transparent);
        --chip-bg: color-mix(in srgb, var(--vscode-badge-background) 22%, transparent);
        --chip-text: var(--vscode-badge-foreground);
        --shadow: 0 20px 60px color-mix(in srgb, black 14%, transparent);
      }

      * {
        box-sizing: border-box;
      }

      body {
        margin: 0;
        min-height: 100vh;
        background: var(--canvas-bg);
        color: var(--panel-strong);
        font-family: var(--vscode-font-family);
      }

      .shell {
        display: grid;
        grid-template-columns: minmax(0, 1.85fr) minmax(280px, 0.95fr);
        gap: 20px;
        min-height: 100vh;
        padding: 20px;
      }

      .canvas-card,
      .meta-card {
        border: 1px solid var(--panel-border);
        background: var(--panel-bg);
        border-radius: 24px;
        box-shadow: var(--shadow);
        overflow: hidden;
        backdrop-filter: blur(16px);
      }

      .canvas-card {
        display: flex;
        flex-direction: column;
        min-height: 0;
      }

      .meta-card {
        padding: 24px;
        overflow: auto;
      }

      .header {
        display: flex;
        flex-wrap: wrap;
        gap: 12px;
        justify-content: space-between;
        align-items: flex-start;
        padding: 22px 24px 0;
      }

      .title-block h1 {
        margin: 0;
        font-size: 1.45rem;
        line-height: 1.15;
        letter-spacing: -0.02em;
      }

      .title-block p {
        margin: 10px 0 0;
        color: var(--panel-muted);
        max-width: 72ch;
        line-height: 1.45;
      }

      .badges {
        display: flex;
        gap: 8px;
        flex-wrap: wrap;
        justify-content: flex-end;
      }

      .badge {
        border-radius: 999px;
        padding: 7px 12px;
        background: var(--chip-bg);
        color: var(--chip-text);
        font-size: 0.82rem;
        font-weight: 700;
        letter-spacing: 0.02em;
        text-transform: uppercase;
      }

      .canvas-wrap {
        min-height: 0;
        overflow: auto;
        padding: 8px 20px 20px;
      }

      .canvas-inner {
        min-width: fit-content;
        display: flex;
        justify-content: center;
        padding: 16px 0 6px;
      }

      svg {
        display: block;
        height: auto;
        max-width: none;
      }

      .panel-section + .panel-section {
        margin-top: 28px;
      }

      .panel-section h2 {
        margin: 0 0 12px;
        font-size: 0.95rem;
        letter-spacing: 0.04em;
        text-transform: uppercase;
        color: var(--panel-muted);
      }

      .focus-card,
      .node-card {
        border: 1px solid var(--panel-border);
        border-radius: 18px;
        background: color-mix(in srgb, var(--vscode-editor-background) 84%, transparent);
        padding: 14px 16px;
      }

      .focus-card {
        background: color-mix(in srgb, var(--accent-soft) 70%, var(--vscode-editor-background));
      }

      .node-list {
        display: grid;
        gap: 12px;
      }

      .node-head {
        display: flex;
        justify-content: space-between;
        gap: 8px;
        align-items: baseline;
      }

      .node-title {
        font-weight: 700;
        line-height: 1.25;
      }

      .node-kind {
        color: var(--panel-muted);
        font-size: 0.82rem;
        white-space: nowrap;
      }

      .node-detail {
        margin: 8px 0 0;
        color: var(--panel-muted);
        line-height: 1.45;
        white-space: pre-wrap;
      }

      .node-source {
        margin-top: 10px;
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 0;
        border: 0;
        background: transparent;
        color: var(--accent);
        cursor: pointer;
        font: inherit;
      }

      .node-source:hover {
        text-decoration: underline;
      }

      .lineage-list {
        list-style: none;
        margin: 0;
        padding: 0;
        display: grid;
        gap: 10px;
      }

      .lineage-list li {
        display: grid;
        grid-template-columns: 32px minmax(0, 1fr);
        gap: 12px;
        align-items: center;
        padding: 10px 12px;
        border: 1px solid var(--panel-border);
        border-radius: 16px;
        background: color-mix(in srgb, var(--vscode-editor-background) 86%, transparent);
      }

      .lineage-index {
        display: inline-grid;
        place-items: center;
        width: 32px;
        height: 32px;
        border-radius: 999px;
        background: var(--accent-soft);
        color: var(--accent);
        font-weight: 700;
      }

      .grid-line {
        stroke: color-mix(in srgb, var(--panel-border) 50%, transparent);
        stroke-width: 1;
      }

      .edge {
        fill: none;
        stroke: color-mix(in srgb, var(--panel-muted) 60%, transparent);
        stroke-width: 2.4;
      }

      .edge.return,
      .edge.revert {
        stroke-width: 3;
      }

      .edge.loop-back,
      .edge.break,
      .edge.continue {
        stroke-dasharray: 7 6;
      }

      .edge-label {
        fill: var(--panel-muted);
        font-size: 12px;
        font-weight: 600;
        text-anchor: middle;
      }

      .node-card-svg rect {
        stroke-width: 1.6;
        fill: color-mix(in srgb, var(--vscode-editor-background) 92%, transparent);
        stroke: color-mix(in srgb, var(--panel-border) 80%, transparent);
      }

      .node-card-svg.focus rect {
        stroke: var(--accent);
        stroke-width: 2.4;
        fill: color-mix(in srgb, var(--accent-soft) 50%, var(--vscode-editor-background));
      }

      .node-card-svg.file rect { fill: #eef2ff; stroke: #4f46e5; }
      .node-card-svg.contract rect { fill: #ecfeff; stroke: #0f766e; }
      .node-card-svg.entry rect { fill: #dcfce7; stroke: #166534; }
      .node-card-svg.exit rect { fill: #fee2e2; stroke: #b91c1c; }
      .node-card-svg.modifier rect { fill: #fff7ed; stroke: #c2410c; }
      .node-card-svg.state rect { fill: #eff6ff; stroke: #1d4ed8; }
      .node-card-svg.call rect { fill: #ecfeff; stroke: #0f766e; }
      .node-card-svg.branch rect { fill: #fef9c3; stroke: #a16207; }
      .node-card-svg.loop rect { fill: #dbeafe; stroke: #1d4ed8; }
      .node-card-svg.terminal rect { fill: #fee2e2; stroke: #b91c1c; }
      .node-card-svg.opaque rect { fill: #e5e7eb; stroke: #4b5563; }
      .node-card-svg.structural rect { fill: #f3f4f6; stroke: #374151; }

      .node-label {
        fill: #101828;
        font-size: 13px;
        font-weight: 800;
      }

      .node-meta {
        fill: #475467;
        font-size: 11px;
      }

      @media (max-width: 1080px) {
        .shell {
          grid-template-columns: 1fr;
        }
      }
    </style>
  </head>
  <body>
    <div class="shell">
      <section class="canvas-card">
        <div class="header">
          <div class="title-block">
            <h1>${escapeHtml(graph.title)}</h1>
            <p>${escapeHtml(snapshot.summary)}</p>
          </div>
          <div class="badges">
            <span class="badge">${graphKindLabel(graph.kind)}</span>
            <span class="badge">${graph.nodes.length} ${pluralize(graph.nodes.length, "node")}</span>
            <span class="badge">${graph.edges.length} ${pluralize(graph.edges.length, "edge")}</span>
          </div>
        </div>
        <div class="canvas-wrap">
          <div class="canvas-inner">
            ${renderSvg(graph, layout)}
          </div>
        </div>
      </section>
      <aside class="meta-card">
        <section class="panel-section">
          <h2>Focus</h2>
          <div class="focus-card">
            <div class="node-title">${escapeHtml(
              snapshot.focusLabel ?? graph.nodes[0]?.label ?? "No focus node"
            )}</div>
            <p class="node-detail">${
              graph.focusNodeId
                ? escapeHtml(
                    graph.nodes.find((node) => node.id === graph.focusNodeId)?.detail ??
                      "Focused node"
                  )
                : "Preview centered on the requested graph target."
            }</p>
          </div>
        </section>
        ${linearizedList}
        <section class="panel-section">
          <h2>Nodes</h2>
          <div class="node-list">
            ${graph.nodes
              .map(
                (node) => `<article class="node-card">
                  <div class="node-head">
                    <div class="node-title">${escapeHtml(node.label)}</div>
                    <div class="node-kind">${escapeHtml(nodeKindLabel(node.kind))}</div>
                  </div>
                  <p class="node-detail">${escapeHtml(node.detail)}</p>
                  ${
                    node.uri
                      ? `<button class="node-source" type="button" data-source-uri="${escapeHtmlAttribute(
                          node.uri
                        )}">Open source <span>${escapeHtml(sourceLabel(node.uri))}</span></button>`
                      : ""
                  }
                </article>`
              )
              .join("")}
          </div>
        </section>
      </aside>
    </div>
    <script nonce="${options.nonce}">
      const vscode = acquireVsCodeApi();
      for (const button of document.querySelectorAll("[data-source-uri]")) {
        button.addEventListener("click", () => {
          const uri = button.getAttribute("data-source-uri");
          if (uri) {
            vscode.postMessage({ type: "openSource", uri });
          }
        });
      }
    </script>
  </body>
</html>`;
}

function edgeClass(kind?: GraphEdgeKind): string {
  return kind ?? "normal";
}

function graphKindLabel(kind: GraphKind): string {
  switch (kind) {
    case "imports":
      return "Imports";
    case "inheritance":
      return "Inheritance";
    case "linearized-inheritance":
      return "Linearized inheritance";
    case "control-flow":
      return "Control flow";
  }
}

function graphSummary(graph: GraphDocumentLike): string {
  const edgeLabel = pluralize(graph.edges.length, "edge");
  const nodeLabel = pluralize(graph.nodes.length, "node");
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

function isBackEdge(kind?: GraphEdgeKind): boolean {
  return kind === "loop-back" || kind === "break" || kind === "continue";
}

function layoutGraph(graph: GraphDocumentLike): LayoutGraph {
  const direction = graph.kind === "control-flow" ? "TD" : "LR";
  const nodes = graph.nodes.map((node) => ({
    ...node,
    height: measureNodeHeight(node),
    width: measureNodeWidth(node),
    x: 0,
    y: 0,
  }));
  const nodeMap = new Map(nodes.map((node) => [node.id, node]));
  const levels = assignLevels(graph);
  const levelBuckets = new Map<number, LayoutNode[]>();
  for (const node of nodes) {
    const level = levels.get(node.id) ?? 0;
    const bucket = levelBuckets.get(level) ?? [];
    bucket.push(node);
    levelBuckets.set(level, bucket);
  }

  const sortedLevels = Array.from(levelBuckets.keys()).sort((a, b) => a - b);
  if (direction === "LR") {
    let x = H_PADDING;
    let height = 0;
    for (const level of sortedLevels) {
      const bucket = levelBuckets.get(level) ?? [];
      const maxWidth = Math.max(...bucket.map((node) => node.width), NODE_WIDTH_MIN);
      let y = V_PADDING;
      for (const node of bucket) {
        node.x = x;
        node.y = y;
        y += node.height + CARD_GAP;
      }
      height = Math.max(height, y - CARD_GAP + V_PADDING);
      x += maxWidth + 88;
    }

    const width = Math.max(x - 88 + H_PADDING, 820);
    return {
      direction,
      edges: graph.edges
        .map((edge) => layoutEdge(edge, nodeMap, direction))
        .filter((edge): edge is LayoutEdge => edge !== undefined),
      height: Math.max(height, 520),
      nodes,
      width,
    };
  }

  let y = V_PADDING;
  let width = 0;
  for (const level of sortedLevels) {
    const bucket = levelBuckets.get(level) ?? [];
    const maxHeight = Math.max(...bucket.map((node) => node.height), 100);
    let x = H_PADDING;
    for (const node of bucket) {
      node.x = x;
      node.y = y;
      x += node.width + CARD_GAP;
    }
    width = Math.max(width, x - CARD_GAP + H_PADDING);
    y += maxHeight + 92;
  }

  return {
    direction,
    edges: graph.edges
      .map((edge) => layoutEdge(edge, nodeMap, direction))
      .filter((edge): edge is LayoutEdge => edge !== undefined),
    height: Math.max(y - 92 + V_PADDING, 620),
    nodes,
    width: Math.max(width, 900),
  };
}

function layoutEdge(
  edge: GraphEdgeLike,
  nodeMap: Map<string, LayoutNode>,
  direction: "LR" | "TD"
): LayoutEdge | undefined {
  const from = nodeMap.get(edge.from);
  const to = nodeMap.get(edge.to);
  if (!from || !to) {
    return undefined;
  }

  if (direction === "LR") {
    const startX = from.x + from.width;
    const startY = from.y + from.height / 2;
    const endX = to.x;
    const endY = to.y + to.height / 2;
    const control = Math.max(48, Math.abs(endX - startX) * 0.38);
    return {
      ...edge,
      labelX: (startX + endX) / 2,
      labelY: (startY + endY) / 2 - 10,
      path: `M ${startX} ${startY} C ${startX + control} ${startY}, ${endX - control} ${endY}, ${endX} ${endY}`,
    };
  }

  const startX = from.x + from.width / 2;
  const startY = from.y + from.height;
  const endX = to.x + to.width / 2;
  const endY = to.y;
  const control = Math.max(48, Math.abs(endY - startY) * 0.35);
  return {
    ...edge,
    labelX: (startX + endX) / 2,
    labelY: (startY + endY) / 2 - 14,
    path: `M ${startX} ${startY} C ${startX} ${startY + control}, ${endX} ${endY - control}, ${endX} ${endY}`,
  };
}

function assignLevels(graph: GraphDocumentLike): Map<string, number> {
  if (graph.kind === "linearized-inheritance") {
    return new Map(graph.nodes.map((node, index) => [node.id, index]));
  }

  const adjacency = new Map<string, string[]>();
  const incoming = new Map<string, number>();
  for (const node of graph.nodes) {
    adjacency.set(node.id, []);
    incoming.set(node.id, 0);
  }

  for (const edge of graph.edges) {
    if (isBackEdge(edge.kind)) {
      continue;
    }
    adjacency.get(edge.from)?.push(edge.to);
    incoming.set(edge.to, (incoming.get(edge.to) ?? 0) + 1);
  }

  const levels = new Map<string, number>();
  const queue: string[] = [];
  const startingIds =
    graph.focusNodeId && adjacency.has(graph.focusNodeId)
      ? [graph.focusNodeId]
      : graph.nodes
          .filter((node) => (incoming.get(node.id) ?? 0) === 0)
          .map((node) => node.id);

  for (const id of startingIds.length > 0
    ? startingIds
    : graph.nodes.slice(0, 1).map((node) => node.id)) {
    levels.set(id, 0);
    queue.push(id);
  }

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current) {
      continue;
    }
    const currentLevel = levels.get(current) ?? 0;
    for (const next of adjacency.get(current) ?? []) {
      if (!levels.has(next)) {
        levels.set(next, currentLevel + 1);
        queue.push(next);
      }
    }
  }

  let fallbackLevel = Math.max(0, ...levels.values());
  for (const node of graph.nodes) {
    if (!levels.has(node.id)) {
      fallbackLevel += 1;
      levels.set(node.id, fallbackLevel);
    }
  }

  return levels;
}

function measureNodeHeight(node: GraphNodeLike): number {
  const detailLines = wrapText(node.detail, 34).slice(0, 3);
  return 68 + detailLines.length * 15;
}

function measureNodeWidth(node: GraphNodeLike): number {
  const longestLine = Math.max(
    node.label.length,
    ...wrapText(node.detail, 32).slice(0, 2).map((line) => line.length),
    0
  );
  return clamp(164 + longestLine * 4.6, NODE_WIDTH_MIN, NODE_WIDTH_MAX);
}

function nodeVisualClass(kind?: GraphNodeKind): string {
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
      return "structural";
  }
}

function nodeKindLabel(kind?: GraphNodeKind): string {
  if (!kind) {
    return "node";
  }
  return kind.replace(/-/g, " ");
}

function pluralize(count: number, singular: string): string {
  return count === 1 ? singular : `${singular}s`;
}

function renderNodeText(node: LayoutNode): string {
  const labelLines = wrapText(node.label, 18);
  const detailLines = wrapText(node.detail, 30).slice(0, 2);
  const lines: string[] = [];
  let y = 28;

  for (const line of labelLines) {
    lines.push(
      `<text class="node-label" x="16" y="${y}">${escapeHtml(line)}</text>`
    );
    y += 16;
  }

  for (const line of detailLines) {
    lines.push(
      `<text class="node-meta" x="16" y="${y}">${escapeHtml(line)}</text>`
    );
    y += 14;
  }

  return lines.join("");
}

function renderSvg(graph: GraphDocumentLike, layout: LayoutGraph): string {
  return `<svg width="${layout.width}" height="${layout.height}" viewBox="0 0 ${layout.width} ${layout.height}" role="img" aria-label="${escapeHtmlAttribute(
    graph.title
  )}">
    <defs>
      <marker id="arrow" markerWidth="10" markerHeight="10" refX="8" refY="5" orient="auto" markerUnits="strokeWidth">
        <path d="M 0 0 L 10 5 L 0 10 z" fill="rgba(120, 132, 158, 0.9)" />
      </marker>
    </defs>
    ${renderBackdrop(layout)}
    ${layout.edges
      .map(
        (edge) => `<g>
          <path class="edge ${edgeClass(edge.kind)}" d="${edge.path}" marker-end="url(#arrow)"></path>
          ${
            edge.label
              ? `<text class="edge-label" x="${edge.labelX}" y="${edge.labelY}">${escapeHtml(
                  edge.label
                )}</text>`
              : ""
          }
        </g>`
      )
      .join("")}
    ${layout.nodes
      .map((node) => {
        const visualClass = nodeVisualClass(node.kind);
        const focusClass = graph.focusNodeId === node.id ? " focus" : "";
        return `<g class="node-card-svg ${visualClass}${focusClass}" transform="translate(${node.x} ${node.y})">
          <rect rx="${NODE_RADIUS}" ry="${NODE_RADIUS}" width="${node.width}" height="${node.height}"></rect>
          ${renderNodeText(node)}
        </g>`;
      })
      .join("")}
  </svg>`;
}

function renderBackdrop(layout: LayoutGraph): string {
  if (layout.direction === "LR") {
    const columns: string[] = [];
    for (let x = H_PADDING; x < layout.width - H_PADDING; x += 120) {
      columns.push(
        `<line class="grid-line" x1="${x}" y1="${V_PADDING / 2}" x2="${x}" y2="${layout.height - V_PADDING / 2}"></line>`
      );
    }
    return columns.join("");
  }

  const rows: string[] = [];
  for (let y = V_PADDING; y < layout.height - V_PADDING; y += 120) {
    rows.push(
      `<line class="grid-line" x1="${H_PADDING / 2}" y1="${y}" x2="${layout.width - H_PADDING / 2}" y2="${y}"></line>`
    );
  }
  return rows.join("");
}

function sourceLabel(uri: string): string {
  try {
    return basename(fileURLToPath(uri));
  } catch {
    return uri;
  }
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

function wrapText(value: string, maxChars: number): string[] {
  const words = value.split(/\s+/).filter(Boolean);
  if (words.length === 0) {
    return [value];
  }

  const lines: string[] = [];
  let current = "";
  for (const word of words) {
    const next = current ? `${current} ${word}` : word;
    if (next.length > maxChars && current) {
      lines.push(current);
      current = word;
    } else {
      current = next;
    }
  }
  if (current) {
    lines.push(current);
  }
  return lines;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function escapeHtmlAttribute(value: string): string {
  return escapeHtml(value).replace(/'/g, "&#39;");
}
