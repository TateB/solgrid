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

export interface GraphRenderBudget {
  canRender: boolean;
  edgeCount: number;
  edgeLimit: number;
  nodeCount: number;
  nodeLimit: number;
  textLength: number;
  textLimit: number;
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
  maxX: number;
  maxY: number;
  minX: number;
  minY: number;
  path: string;
}

interface LayoutGraph {
  direction: "LR" | "TD";
  edges: LayoutEdge[];
  height: number;
  nodes: LayoutNode[];
  viewBoxX: number;
  viewBoxY: number;
  width: number;
}

interface RenderOptions {
  cspSource: string;
  nonce: string;
}

const CARD_GAP = 24;
const EDGE_LABEL_MAX_CHARS = 18;
const EDGE_LABEL_WIDTH_MAX = 144;
const EDGE_ROUTE_MARGIN = 14;
export const GRAPH_RENDER_EDGE_LIMIT = 4_000;
export const GRAPH_RENDER_NODE_LIMIT = 2_000;
export const GRAPH_RENDER_TEXT_LIMIT = 500_000;
export const GRAPH_MANUAL_SCALE_MIN = 0.2;
export const GRAPH_SCALE_MAX = 1.8;
const H_PADDING = 40;
const NODE_DETAIL_LINES_MAX = 2;
const NODE_DETAIL_WRAP = 30;
const NODE_LABEL_LINES_MAX = 3;
const NODE_LABEL_WRAP = 28;
const NODE_RADIUS = 18;
const NODE_SOURCE_ACTION_GAP = 10;
const NODE_SOURCE_ACTION_HEIGHT = 22;
const NODE_SOURCE_ACTION_WIDTH = 88;
const NODE_WIDTH_MAX = 320;
const NODE_WIDTH_MIN = 180;
const SOURCE_LABEL_MAX_CHARS = 80;
const SOURCE_LABEL_SUMMARY_LIMIT = 3;
const V_PADDING = 40;
const SAME_RANK_EDGE_GAP = 6;
const TD_BRANCH_LANE_GAP = 56;
const TD_LEVEL_GAP = 92;
const TD_NODE_GAP = 32;

const GRAPH_KINDS = new Set<GraphKind>([
  "imports",
  "inheritance",
  "linearized-inheritance",
  "control-flow",
]);
const GRAPH_NODE_KINDS = new Set<GraphNodeKind>([
  "file",
  "contract",
  "entry",
  "exit",
  "modifier",
  "declaration",
  "assignment",
  "call",
  "emit",
  "branch",
  "loop",
  "loop-next",
  "terminal-return",
  "terminal-revert",
  "control-transfer",
  "assembly",
  "try",
  "catch",
  "block",
  "statement",
]);
const GRAPH_EDGE_KINDS = new Set<GraphEdgeKind>([
  "imports",
  "inherits",
  "precedes",
  "normal",
  "branch-true",
  "branch-false",
  "loop-back",
  "return",
  "revert",
  "break",
  "continue",
]);

export function isGraphDocumentLike(value: unknown): value is GraphDocumentLike {
  if (!value || typeof value !== "object") {
    return false;
  }
  const graph = value as Partial<GraphDocumentLike>;
  if (
    typeof graph.title !== "string" ||
    !GRAPH_KINDS.has(graph.kind as GraphKind) ||
    !Array.isArray(graph.nodes) ||
    !Array.isArray(graph.edges) ||
    (graph.focusNodeId !== undefined && typeof graph.focusNodeId !== "string")
  ) {
    return false;
  }

  const nodeIds = new Set<string>();
  for (const node of graph.nodes) {
    if (
      !node ||
      typeof node !== "object" ||
      typeof node.id !== "string" ||
      typeof node.label !== "string" ||
      typeof node.detail !== "string" ||
      (node.uri !== undefined && typeof node.uri !== "string") ||
      (node.kind !== undefined && !GRAPH_NODE_KINDS.has(node.kind)) ||
      nodeIds.has(node.id)
    ) {
      return false;
    }
    nodeIds.add(node.id);
  }

  if (graph.focusNodeId !== undefined && !nodeIds.has(graph.focusNodeId)) {
    return false;
  }
  return graph.edges.every(
    (edge) =>
      edge !== null &&
      typeof edge === "object" &&
      typeof edge.from === "string" &&
      typeof edge.to === "string" &&
      nodeIds.has(edge.from) &&
      nodeIds.has(edge.to) &&
      (edge.label === undefined || typeof edge.label === "string") &&
      (edge.kind === undefined || GRAPH_EDGE_KINDS.has(edge.kind))
  );
}

export function buildGraphPreviewSnapshot(
  graph: GraphDocumentLike
): GraphPreviewSnapshot {
  const displayGraph = graphForDisplay(graph);
  const focusLabel = displayGraph.focusNodeId
    ? displayGraph.nodes.find((node) => node.id === displayGraph.focusNodeId)?.label
    : undefined;

  return {
    edgeCount: displayGraph.edges.length,
    focusLabel,
    kind: displayGraph.kind,
    nodeLabels: displayGraph.nodes.map((node) => node.label),
    summary: graphSummary(displayGraph),
    title: displayGraph.title,
  };
}

export function assessGraphRenderBudget(
  graph: GraphDocumentLike
): GraphRenderBudget {
  return assessDisplayedGraphRenderBudget(graphForDisplay(graph));
}

export function calculateGraphFitScale(
  viewportWidth: number,
  viewportHeight: number,
  graphWidth: number,
  graphHeight: number,
  includeHeight: boolean,
  viewportInset = 12
): number {
  if (
    !Number.isFinite(viewportWidth) ||
    !Number.isFinite(viewportHeight) ||
    !Number.isFinite(graphWidth) ||
    !Number.isFinite(graphHeight) ||
    viewportWidth <= 0 ||
    viewportHeight <= 0 ||
    graphWidth <= 0 ||
    graphHeight <= 0
  ) {
    return 1;
  }

  const inset = Math.max(0, viewportInset);
  const availableWidth = Math.max(1, viewportWidth - inset);
  const availableHeight = Math.max(1, viewportHeight - inset);
  const widthScale = availableWidth / graphWidth;
  const heightScale = availableHeight / graphHeight;
  return Math.max(
    Number.MIN_VALUE,
    Math.min(widthScale, includeHeight ? heightScale : 1, 1)
  );
}

export function clampManualGraphScale(
  value: number,
  minimum = 0.2,
  maximum = 1.8
): number {
  if (!Number.isFinite(value)) {
    return 1;
  }
  return Math.min(maximum, Math.max(minimum, value));
}

export function renderGraphWebviewHtml(
  graph: GraphDocumentLike,
  options: RenderOptions
): string {
  const displayGraph = graphForDisplay(graph);
  const renderBudget = assessDisplayedGraphRenderBudget(displayGraph);
  if (!renderBudget.canRender) {
    throw new RangeError(
      `Graph exceeds the interactive render budget (${renderBudget.nodeCount} nodes, ${renderBudget.edgeCount} relationships, ${renderBudget.textLength} text characters).`
    );
  }
  const layout = layoutGraph(displayGraph);
  const snapshot = buildGraphPreviewSnapshot(displayGraph);

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
        --panel-bg: var(--vscode-editor-background);
        --panel-bg: color-mix(in srgb, var(--vscode-editor-background) 94%, var(--vscode-sideBar-background) 6%);
        --panel-border: var(--vscode-panel-border);
        --panel-border: color-mix(in srgb, var(--vscode-panel-border) 72%, transparent);
        --panel-muted: var(--vscode-descriptionForeground);
        --panel-muted: color-mix(in srgb, var(--vscode-descriptionForeground) 82%, transparent);
        --panel-strong: var(--vscode-foreground);
        --canvas-bg: var(--vscode-editor-background);
        --accent: var(--vscode-textLink-foreground);
        --accent: color-mix(in srgb, var(--vscode-textLink-foreground) 86%, white 14%);
        --accent-soft: transparent;
        --accent-soft: color-mix(in srgb, var(--vscode-textLink-foreground) 16%, transparent);
        --chip-bg: var(--vscode-badge-background);
        --chip-bg: color-mix(in srgb, var(--vscode-badge-background) 22%, transparent);
        --chip-text: var(--vscode-badge-foreground);
        --edge-default: var(--vscode-descriptionForeground);
        --edge-false: #b45309;
        --edge-true: #15803d;
        --node-label-fill: #101828;
        --node-meta-fill: #475467;
        --node-file-bg: #eef2ff;
        --node-file-stroke: #4f46e5;
        --node-contract-bg: #ecfeff;
        --node-contract-stroke: #0f766e;
        --node-entry-bg: #dcfce7;
        --node-entry-stroke: #166534;
        --node-exit-bg: #fee2e2;
        --node-exit-stroke: #b91c1c;
        --node-modifier-bg: #fff7ed;
        --node-modifier-stroke: #c2410c;
        --node-state-bg: #eff6ff;
        --node-state-stroke: #1d4ed8;
        --node-call-bg: #ecfeff;
        --node-call-stroke: #0f766e;
        --node-branch-bg: #fef9c3;
        --node-branch-stroke: #a16207;
        --node-loop-bg: #dbeafe;
        --node-loop-stroke: #1d4ed8;
        --node-terminal-bg: #fee2e2;
        --node-terminal-stroke: #b91c1c;
        --node-opaque-bg: #e5e7eb;
        --node-opaque-stroke: #4b5563;
        --node-structural-bg: #f3f4f6;
        --node-structural-stroke: #374151;
      }

      body.vscode-light {
        --panel-bg: var(--vscode-editor-background);
        --panel-bg: color-mix(in srgb, var(--vscode-editor-background) 96%, var(--vscode-sideBar-background) 4%);
        --panel-border: var(--vscode-panel-border);
        --panel-border: color-mix(in srgb, var(--vscode-panel-border) 80%, #d0d7de);
        --panel-muted: var(--vscode-descriptionForeground);
        --panel-muted: color-mix(in srgb, var(--vscode-descriptionForeground) 88%, #57606a);
        --accent: var(--vscode-textLink-foreground);
        --accent-soft: white;
        --accent-soft: color-mix(in srgb, var(--vscode-textLink-foreground) 12%, white);
        --chip-bg: var(--vscode-badge-background);
        --chip-bg: color-mix(in srgb, var(--vscode-badge-background) 12%, white);
        --chip-text: var(--vscode-foreground);
        --edge-false: #9a6500;
        --edge-true: #2f7d4a;
        --node-file-bg: #f6f8ff;
        --node-file-stroke: #5967d8;
        --node-contract-bg: #effcf9;
        --node-contract-stroke: #16806f;
        --node-entry-bg: #e9f9ef;
        --node-entry-stroke: #2f7d4a;
        --node-exit-bg: #fff0f0;
        --node-exit-stroke: #bd3d3d;
        --node-modifier-bg: #fff6eb;
        --node-modifier-stroke: #b85f18;
        --node-state-bg: #f2f7ff;
        --node-state-stroke: #3468c9;
        --node-call-bg: #effcf9;
        --node-call-stroke: #16806f;
        --node-branch-bg: #fff9d7;
        --node-branch-stroke: #9a7800;
        --node-loop-bg: #eef5ff;
        --node-loop-stroke: #3468c9;
        --node-terminal-bg: #fff0f0;
        --node-terminal-stroke: #bd3d3d;
        --node-opaque-bg: #f0f2f5;
        --node-opaque-stroke: #57606a;
        --node-structural-bg: #f6f8fa;
        --node-structural-stroke: #57606a;
      }

      * {
        box-sizing: border-box;
      }

      body {
        margin: 0;
        height: 100vh;
        overflow: hidden;
        background: var(--canvas-bg);
        color: var(--panel-strong);
        font-family: var(--vscode-font-family);
      }

      body.vscode-light,
      body.vscode-high-contrast-light {
        color-scheme: light;
      }

      body.vscode-dark,
      body.vscode-high-contrast {
        color-scheme: dark;
      }

      .shell {
        display: flex;
        height: 100vh;
        min-height: 0;
        overflow: hidden;
        padding: 0;
      }

      .canvas-card {
        background: var(--panel-bg);
        overflow: hidden;
        min-height: 0;
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-width: 0;
        background: var(--canvas-bg);
      }

      .header {
        display: flex;
        flex-wrap: wrap;
        gap: 12px;
        justify-content: space-between;
        align-items: flex-start;
        padding: 18px 18px 0;
        flex: 0 0 auto;
      }

      .title-block {
        flex: 1 1 320px;
        min-width: 0;
      }

      .title-block h1 {
        display: -webkit-box;
        margin: 0;
        overflow: hidden;
        overflow-wrap: anywhere;
        font-size: 1.2rem;
        line-height: 1.2;
        letter-spacing: 0;
        -webkit-box-orient: vertical;
        -webkit-line-clamp: 2;
      }

      .title-block p {
        display: -webkit-box;
        margin: 10px 0 0;
        overflow: hidden;
        color: var(--panel-muted);
        max-width: 72ch;
        line-height: 1.45;
        -webkit-box-orient: vertical;
        -webkit-line-clamp: 3;
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

      .graph-toolbar {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: 6px;
        padding: 10px 18px 0;
        flex: 0 0 auto;
      }

      .zoom-button {
        display: inline-grid;
        place-items: center;
        min-width: 32px;
        height: 28px;
        border: 1px solid var(--panel-border);
        border-radius: 6px;
        background: var(--vscode-editor-background);
        background: color-mix(in srgb, var(--vscode-editor-background) 86%, transparent);
        color: var(--panel-strong);
        cursor: pointer;
        font: inherit;
        font-size: 0.82rem;
        font-weight: 700;
        line-height: 1;
      }

      .zoom-button:hover {
        border-color: var(--accent);
        color: var(--accent);
      }

      .zoom-button:focus-visible,
      .graph-details > summary:focus-visible,
      .source-button:focus-visible {
        outline: 2px solid var(--vscode-focusBorder, var(--vscode-contrastActiveBorder, Highlight));
        outline-offset: 2px;
      }

      .zoom-readout {
        min-width: 48px;
        color: var(--panel-muted);
        font-size: 0.82rem;
        font-variant-numeric: tabular-nums;
        text-align: right;
      }

      .graph-details {
        margin: 10px 18px 0;
        max-height: min(42vh, 360px);
        overflow: auto;
        border: 1px solid var(--panel-border);
        border-radius: 7px;
        background: var(--panel-bg);
        color: var(--panel-strong);
        flex: 0 1 auto;
      }

      .graph-details > summary {
        padding: 7px 10px;
        color: var(--panel-muted);
        cursor: pointer;
        font-size: 0.85rem;
        font-weight: 700;
      }

      .graph-details[open] > summary {
        border-bottom: 1px solid var(--panel-border);
        color: var(--panel-strong);
      }

      .graph-details-content {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(min(100%, 260px), 1fr));
        gap: 16px;
        padding: 12px;
      }

      .graph-details-section h2,
      .graph-details-section h3 {
        margin: 0 0 8px;
        font-size: 0.9rem;
      }

      .graph-details-section ul {
        display: grid;
        gap: 8px;
        margin: 0;
        padding-left: 20px;
      }

      .graph-details-section li {
        line-height: 1.4;
      }

      .node-kind-cue {
        display: inline-block;
        margin-right: 5px;
        color: var(--panel-muted);
        font-size: 0.74rem;
        font-weight: 800;
        letter-spacing: 0.04em;
        text-transform: uppercase;
      }

      .node-detail {
        display: block;
        margin-top: 2px;
        color: var(--panel-muted);
      }

      .source-button {
        border: 1px solid var(--panel-border);
        border-radius: 5px;
        padding: 4px 7px;
        background: var(--vscode-button-secondaryBackground, var(--vscode-editor-background));
        color: var(--vscode-button-secondaryForeground, var(--panel-strong));
        cursor: pointer;
        font: inherit;
      }

      .source-button:hover {
        background: var(--vscode-button-secondaryHoverBackground, var(--accent-soft));
      }

      .viewport-instructions {
        position: absolute;
        width: 1px;
        height: 1px;
        padding: 0;
        margin: -1px;
        overflow: hidden;
        clip: rect(0, 0, 0, 0);
        white-space: nowrap;
        border: 0;
      }

      .canvas-wrap {
        flex: 1 1 auto;
        min-height: 0;
        overflow: auto;
        overscroll-behavior: contain;
        padding: 12px 18px 18px;
        cursor: grab;
        scrollbar-gutter: stable;
        touch-action: none;
      }

      .canvas-wrap:focus-visible {
        outline: 2px solid var(--vscode-focusBorder, var(--vscode-contrastActiveBorder, Highlight));
        outline-offset: -3px;
      }

      .canvas-wrap.is-panning {
        cursor: grabbing;
        user-select: none;
      }

      .canvas-inner {
        display: flex;
        justify-content: center;
        align-items: flex-start;
        min-width: 100%;
        min-height: 100%;
        width: max-content;
        padding: 0;
      }

      .graph-stage {
        position: relative;
        width: ${layout.width}px;
        height: ${layout.height}px;
      }

      svg {
        display: block;
        height: auto;
        max-width: none;
        pointer-events: auto;
        transform-origin: top left;
      }

      .grid-line {
        stroke: var(--panel-border);
        stroke: color-mix(in srgb, var(--panel-border) 50%, transparent);
        stroke-width: 1;
      }

      .edge {
        fill: none;
        stroke: var(--edge-default);
        stroke-width: 2.4;
      }

      .arrow-head {
        fill: var(--edge-default);
      }

      .arrow-head.branch-true {
        fill: var(--edge-true);
      }

      .arrow-head.branch-false {
        fill: var(--edge-false);
      }

      .edge.branch-true {
        stroke: var(--edge-true);
      }

      .edge.branch-false {
        stroke: var(--edge-false);
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
        dominant-baseline: middle;
        text-anchor: middle;
      }

      .edge-label-pill .edge-label-box {
        fill: var(--vscode-editor-background);
        fill: color-mix(in srgb, var(--vscode-editor-background) 88%, transparent);
        stroke: var(--edge-default);
        stroke-width: 1;
      }

      .edge-label-pill.branch-true .edge-label {
        fill: var(--edge-true);
      }

      .edge-label-pill.branch-true .edge-label-box {
        fill: var(--vscode-editor-background);
        fill: color-mix(in srgb, var(--edge-true) 12%, var(--vscode-editor-background));
        stroke: var(--edge-true);
      }

      .edge-label-pill.branch-false .edge-label {
        fill: var(--edge-false);
      }

      .edge-label-pill.branch-false .edge-label-box {
        fill: var(--vscode-editor-background);
        fill: color-mix(in srgb, var(--edge-false) 12%, var(--vscode-editor-background));
        stroke: var(--edge-false);
      }

      .node-card-svg > rect {
        stroke-width: 1.6;
        fill: var(--vscode-editor-background);
        fill: color-mix(in srgb, var(--vscode-editor-background) 92%, transparent);
        stroke: var(--panel-border);
        stroke: color-mix(in srgb, var(--panel-border) 80%, transparent);
      }

      .node-card-svg.file > rect { fill: var(--node-file-bg); stroke: var(--node-file-stroke); }
      .node-card-svg.contract > rect { fill: var(--node-contract-bg); stroke: var(--node-contract-stroke); }
      .node-card-svg.entry > rect { fill: var(--node-entry-bg); stroke: var(--node-entry-stroke); }
      .node-card-svg.exit > rect { fill: var(--node-exit-bg); stroke: var(--node-exit-stroke); }
      .node-card-svg.modifier > rect { fill: var(--node-modifier-bg); stroke: var(--node-modifier-stroke); }
      .node-card-svg.state > rect { fill: var(--node-state-bg); stroke: var(--node-state-stroke); }
      .node-card-svg.call > rect { fill: var(--node-call-bg); stroke: var(--node-call-stroke); }
      .node-card-svg.branch > rect { fill: var(--node-branch-bg); stroke: var(--node-branch-stroke); }
      .node-card-svg.loop > rect { fill: var(--node-loop-bg); stroke: var(--node-loop-stroke); }
      .node-card-svg.terminal > rect { fill: var(--node-terminal-bg); stroke: var(--node-terminal-stroke); }
      .node-card-svg.opaque > rect { fill: var(--node-opaque-bg); stroke: var(--node-opaque-stroke); }
      .node-card-svg.structural > rect { fill: var(--node-structural-bg); stroke: var(--node-structural-stroke); }

      .node-card-svg.focus > rect {
        stroke: var(--accent);
        stroke-width: 2.4;
        fill: var(--vscode-editor-background);
        fill: color-mix(in srgb, var(--accent-soft) 50%, var(--vscode-editor-background));
      }

      .node-card-svg text,
      .node-card-svg > rect,
      .edge,
      .edge-label,
      .edge-label-pill,
      .grid-line {
        pointer-events: none;
      }

      .node-label {
        fill: var(--node-label-fill);
        font-size: 13px;
        font-weight: 800;
      }

      .node-meta {
        fill: var(--node-meta-fill);
        font-size: 11px;
      }

      .node-kind {
        fill: var(--node-meta-fill);
        font-size: 9px;
        font-weight: 800;
        letter-spacing: 0.08em;
        text-transform: uppercase;
      }

      .source-chip {
        cursor: pointer;
        pointer-events: all;
      }

      .source-chip rect {
        fill: white;
        fill: color-mix(in srgb, white 72%, transparent);
        stroke: var(--accent);
        stroke: color-mix(in srgb, var(--accent) 72%, transparent);
        stroke-width: 1;
      }

      .source-chip text {
        fill: var(--node-label-fill);
        font-size: 10px;
        font-weight: 800;
        pointer-events: none;
      }

      .source-chip:hover rect,
      .source-chip:focus rect {
        stroke: var(--accent);
        fill: white;
        fill: color-mix(in srgb, white 86%, var(--accent-soft));
      }

      body.vscode-high-contrast .node-card-svg > rect,
      body.vscode-high-contrast-light .node-card-svg > rect {
        fill: var(--vscode-editor-background);
        stroke: var(--vscode-contrastActiveBorder, var(--vscode-foreground));
        stroke-width: 2px;
      }

      body.vscode-high-contrast .node-card-svg.focus > rect,
      body.vscode-high-contrast-light .node-card-svg.focus > rect {
        stroke: var(--vscode-focusBorder, var(--vscode-contrastActiveBorder, Highlight));
        stroke-width: 3px;
        stroke-dasharray: 7 3;
      }

      body.vscode-high-contrast .node-label,
      body.vscode-high-contrast .node-meta,
      body.vscode-high-contrast .node-kind,
      body.vscode-high-contrast-light .node-label,
      body.vscode-high-contrast-light .node-meta,
      body.vscode-high-contrast-light .node-kind {
        fill: var(--vscode-foreground);
      }

      body.vscode-high-contrast .edge,
      body.vscode-high-contrast-light .edge {
        stroke: var(--vscode-foreground);
      }

      body.vscode-high-contrast .arrow-head,
      body.vscode-high-contrast-light .arrow-head {
        fill: var(--vscode-foreground);
      }

      body.vscode-high-contrast .edge-label-pill .edge-label-box,
      body.vscode-high-contrast .source-chip rect,
      body.vscode-high-contrast-light .edge-label-pill .edge-label-box,
      body.vscode-high-contrast-light .source-chip rect {
        fill: var(--vscode-editor-background);
        stroke: var(--vscode-foreground);
      }

      body.vscode-high-contrast .edge-label-pill .edge-label,
      body.vscode-high-contrast .source-chip text,
      body.vscode-high-contrast-light .edge-label-pill .edge-label,
      body.vscode-high-contrast-light .source-chip text {
        fill: var(--vscode-foreground);
      }

      @media (forced-colors: active) {
        .node-card-svg.node-card-svg > rect,
        .edge-label-pill.edge-label-pill .edge-label-box,
        .source-chip.source-chip rect {
          fill: Canvas;
          stroke: CanvasText;
        }

        .node-label,
        .node-meta,
        .node-kind,
        .edge-label-pill.edge-label-pill .edge-label,
        .source-chip text {
          fill: CanvasText;
        }

        .edge.edge,
        .grid-line {
          stroke: CanvasText;
        }

        .arrow-head.arrow-head {
          fill: CanvasText;
        }

        .node-card-svg.focus > rect {
          stroke: Highlight;
          stroke-width: 3px;
          stroke-dasharray: 7 3;
        }
      }

      @media (max-width: 700px) {
        .header {
          gap: 8px;
          padding: 14px 14px 0;
        }

        .title-block h1 {
          font-size: 1.05rem;
        }

        .title-block p {
          display: -webkit-box;
          margin-top: 6px;
          overflow: hidden;
          -webkit-box-orient: vertical;
          -webkit-line-clamp: 2;
        }

        .badges {
          justify-content: flex-start;
        }

        .graph-toolbar {
          padding: 8px 14px 0;
        }

        .graph-details {
          margin: 8px 14px 0;
        }

        .canvas-wrap {
          padding: 10px 14px 14px;
        }
      }
    </style>
  </head>
  <body>
    <div class="shell">
      <section class="canvas-card">
        <div class="header">
          <div class="title-block">
            <h1 title="${escapeHtmlAttribute(graph.title)}">${escapeHtml(
              graph.title
            )}</h1>
            <p title="${escapeHtmlAttribute(snapshot.summary)}">${escapeHtml(
              snapshot.summary
            )}</p>
          </div>
          <div class="badges">
            <span class="badge">${graphKindLabel(displayGraph.kind)}</span>
            <span class="badge">${displayGraph.nodes.length} ${pluralize(displayGraph.nodes.length, "node")}</span>
            <span class="badge">${displayGraph.edges.length} ${pluralize(displayGraph.edges.length, "edge")}</span>
          </div>
        </div>
        <div class="graph-toolbar" role="toolbar" aria-label="Graph view controls">
          <button class="zoom-button" type="button" data-zoom-action="out" title="Zoom out" aria-label="Zoom out">-</button>
          <button class="zoom-button" type="button" data-zoom-action="reset" title="Reset zoom" aria-label="Reset zoom">1:1</button>
          <button class="zoom-button" type="button" data-zoom-action="fit" title="Fit graph" aria-label="Fit graph">Fit</button>
          <button class="zoom-button" type="button" data-zoom-action="in" title="Zoom in" aria-label="Zoom in">+</button>
          <span class="zoom-readout" aria-live="polite">100%</span>
        </div>
        ${renderAccessibleGraphDetails(displayGraph)}
        <p class="viewport-instructions" id="graph-viewport-instructions">Use the arrow keys to scroll the graph. Page Up and Page Down scroll by a viewport. Home moves to the start and End moves to the end.</p>
        <div class="canvas-wrap" tabindex="0" role="region" aria-label="${escapeHtmlAttribute(
          `${graph.title} interactive graph viewport`
        )}" aria-describedby="graph-viewport-instructions">
          <div class="canvas-inner">
            <div class="graph-stage" data-graph-stage data-graph-width="${layout.width}" data-graph-height="${layout.height}">
              ${renderSvg(displayGraph, layout)}
            </div>
          </div>
        </div>
      </section>
    </div>
    <script nonce="${options.nonce}">
      const vscode = typeof acquireVsCodeApi === "function"
        ? acquireVsCodeApi()
        : { postMessage: () => {} };
      const graphStage = document.querySelector("[data-graph-stage]");
      const graphSvg = graphStage?.querySelector("svg");
      const canvasWrap = document.querySelector(".canvas-wrap");
      const zoomReadout = document.querySelector(".zoom-readout");
      const graphWidth = Number(graphStage?.getAttribute("data-graph-width") ?? 0);
      const graphHeight = Number(graphStage?.getAttribute("data-graph-height") ?? 0);
      let graphScale = 1;
      let autoFitMode = "width";
      let panStart = null;
      const KEYBOARD_SCROLL_STEP = 48;
      const WHEEL_LINE_HEIGHT = 16;
      const WHEEL_ZOOM_DELTA_MAX = 120;
      const WHEEL_ZOOM_SENSITIVITY = 0.002;
      const TRACKPAD_ZOOM_SENSITIVITY = 0.004;
      const GRAPH_MANUAL_SCALE_MIN = ${GRAPH_MANUAL_SCALE_MIN};
      const GRAPH_SCALE_MAX = ${GRAPH_SCALE_MAX};
      const calculateGraphFitScale = ${calculateGraphFitScale.toString()};
      const clampManualGraphScale = ${clampManualGraphScale.toString()};

      function clampRenderedScale(value) {
        if (!Number.isFinite(value) || value <= 0) {
          return 1;
        }
        return Math.min(GRAPH_SCALE_MAX, Math.max(Number.MIN_VALUE, value));
      }

      function fitScale({ includeHeight = false } = {}) {
        if (!canvasWrap || !graphWidth || !graphHeight) {
          return 1;
        }
        return calculateGraphFitScale(
          canvasWrap.clientWidth,
          canvasWrap.clientHeight,
          graphWidth,
          graphHeight,
          includeHeight
        );
      }

      function setGraphScale(
        nextScale,
        { preserveCenter = true, anchorClientX = null, anchorClientY = null } = {}
      ) {
        if (!graphStage || !graphSvg || !canvasWrap || !graphWidth || !graphHeight) {
          return;
        }

        const viewportRect = canvasWrap.getBoundingClientRect();
        const anchorX =
          anchorClientX === null
            ? preserveCenter
              ? canvasWrap.clientWidth / 2
              : null
            : anchorClientX - viewportRect.left;
        const anchorY =
          anchorClientY === null
            ? preserveCenter
              ? canvasWrap.clientHeight / 2
              : null
            : anchorClientY - viewportRect.top;
        const graphAnchorX =
          anchorX === null ? null : (canvasWrap.scrollLeft + anchorX) / graphScale;
        const graphAnchorY =
          anchorY === null ? null : (canvasWrap.scrollTop + anchorY) / graphScale;
        graphScale = clampRenderedScale(nextScale);
        graphStage.style.width = graphWidth * graphScale + "px";
        graphStage.style.height = graphHeight * graphScale + "px";
        graphSvg.style.transform = "scale(" + graphScale + ")";
        if (zoomReadout) {
          const scalePercent = graphScale * 100;
          zoomReadout.textContent =
            scalePercent > 0 && scalePercent < 1
              ? "<1%"
              : Math.round(scalePercent) + "%";
        }
        if (graphAnchorX !== null && graphAnchorY !== null && anchorX !== null && anchorY !== null) {
          canvasWrap.scrollLeft = graphAnchorX * graphScale - anchorX;
          canvasWrap.scrollTop = graphAnchorY * graphScale - anchorY;
        }
      }

      function wheelZoomFactor(event) {
        const deltaMultiplier =
          event.deltaMode === WheelEvent.DOM_DELTA_LINE
            ? WHEEL_LINE_HEIGHT
            : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
              ? Math.max(canvasWrap?.clientHeight ?? 0, WHEEL_LINE_HEIGHT)
              : 1;
        const deltaPixels = event.deltaY * deltaMultiplier;
        const clampedDelta = Math.max(
          -WHEEL_ZOOM_DELTA_MAX,
          Math.min(WHEEL_ZOOM_DELTA_MAX, deltaPixels)
        );
        const sensitivity =
          event.deltaMode === WheelEvent.DOM_DELTA_PIXEL
            ? TRACKPAD_ZOOM_SENSITIVITY
            : WHEEL_ZOOM_SENSITIVITY;
        return Math.exp(-clampedDelta * sensitivity);
      }

      function setManualGraphScale(nextScale, options = {}) {
        autoFitMode = null;
        if (graphScale < GRAPH_MANUAL_SCALE_MIN && nextScale <= graphScale) {
          setGraphScale(graphScale, options);
          return;
        }
        setGraphScale(clampManualGraphScale(nextScale), options);
      }

      function scrollGraphViewport(event) {
        if (!canvasWrap) {
          return;
        }
        const pageStep = Math.max(KEYBOARD_SCROLL_STEP, canvasWrap.clientHeight * 0.9);
        let left = canvasWrap.scrollLeft;
        let top = canvasWrap.scrollTop;
        switch (event.key) {
          case "ArrowLeft":
            left -= KEYBOARD_SCROLL_STEP;
            break;
          case "ArrowRight":
            left += KEYBOARD_SCROLL_STEP;
            break;
          case "ArrowUp":
            top -= KEYBOARD_SCROLL_STEP;
            break;
          case "ArrowDown":
            top += KEYBOARD_SCROLL_STEP;
            break;
          case "PageUp":
            top -= pageStep;
            break;
          case "PageDown":
            top += pageStep;
            break;
          case "Home":
            left = 0;
            top = 0;
            break;
          case "End":
            left = canvasWrap.scrollWidth;
            top = canvasWrap.scrollHeight;
            break;
          default:
            return;
        }
        event.preventDefault();
        canvasWrap.scrollTo({ left, top, behavior: "auto" });
      }

      function beginPan(event) {
        if (!canvasWrap || event.button !== 0) {
          return;
        }
        const target = event.target;
        if (target instanceof Element && target.closest("[data-source-uri], button, a")) {
          return;
        }
        panStart = {
          clientX: event.clientX,
          clientY: event.clientY,
          scrollLeft: canvasWrap.scrollLeft,
          scrollTop: canvasWrap.scrollTop,
        };
        canvasWrap.classList.add("is-panning");
        canvasWrap.setPointerCapture?.(event.pointerId);
      }

      function updatePan(event) {
        if (!canvasWrap || !panStart) {
          return;
        }
        event.preventDefault();
        canvasWrap.scrollLeft = panStart.scrollLeft - (event.clientX - panStart.clientX);
        canvasWrap.scrollTop = panStart.scrollTop - (event.clientY - panStart.clientY);
      }

      function endPan(event) {
        if (!canvasWrap || !panStart) {
          return;
        }
        panStart = null;
        canvasWrap.classList.remove("is-panning");
        canvasWrap.releasePointerCapture?.(event.pointerId);
      }

      if (canvasWrap) {
        canvasWrap.addEventListener("pointerdown", beginPan);
        canvasWrap.addEventListener("pointermove", updatePan);
        canvasWrap.addEventListener("pointerup", endPan);
        canvasWrap.addEventListener("pointercancel", endPan);
        canvasWrap.addEventListener("keydown", scrollGraphViewport);
        canvasWrap.addEventListener("dblclick", (event) => {
          event.preventDefault();
          setManualGraphScale(graphScale * 1.25, {
            anchorClientX: event.clientX,
            anchorClientY: event.clientY,
          });
        });
        canvasWrap.addEventListener(
          "wheel",
          (event) => {
            if (!event.ctrlKey && !event.metaKey) {
              return;
            }
            event.preventDefault();
            const zoomFactor = wheelZoomFactor(event);
            setManualGraphScale(graphScale * zoomFactor, {
              anchorClientX: event.clientX,
              anchorClientY: event.clientY,
            });
          },
          { passive: false }
        );
      }

      for (const button of document.querySelectorAll("[data-zoom-action]")) {
        button.addEventListener("click", () => {
          const action = button.getAttribute("data-zoom-action");
          if (action === "in") {
            setManualGraphScale(graphScale + 0.1);
          } else if (action === "out") {
            setManualGraphScale(graphScale - 0.1);
          } else if (action === "reset") {
            setManualGraphScale(1);
          } else if (action === "fit") {
            autoFitMode = "all";
            setGraphScale(fitScale({ includeHeight: true }), { preserveCenter: false });
          }
        });
      }

      requestAnimationFrame(() => {
        setGraphScale(fitScale(), { preserveCenter: false });
      });

      window.addEventListener("resize", () => {
        if (autoFitMode !== null) {
          setGraphScale(
            fitScale({ includeHeight: autoFitMode === "all" }),
            { preserveCenter: false }
          );
        }
      });

      function openSourceFromElement(element) {
        const uri = element.getAttribute("data-source-uri");
        if (uri) {
          vscode.postMessage({ type: "openSource", uri });
        }
      }

      for (const button of document.querySelectorAll("[data-source-uri]")) {
        button.addEventListener("click", (event) => {
          event.stopPropagation();
          openSourceFromElement(button);
        });
      }
    </script>
  </body>
</html>`;
}

function edgeClass(kind?: GraphEdgeKind): string {
  return kind ?? "normal";
}

function edgeMarkerId(kind?: GraphEdgeKind): string {
  switch (kind) {
    case "branch-true":
      return "arrow-branch-true";
    case "branch-false":
      return "arrow-branch-false";
    default:
      return "arrow-default";
  }
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

function nodeKindLabel(kind?: GraphNodeKind): string {
  switch (kind) {
    case "file":
      return "File";
    case "contract":
      return "Contract";
    case "entry":
      return "Entry";
    case "exit":
      return "Exit";
    case "modifier":
      return "Modifier";
    case "declaration":
      return "Declaration";
    case "assignment":
      return "Assignment";
    case "call":
      return "Call";
    case "emit":
      return "Emission";
    case "branch":
      return "Branch";
    case "loop":
      return "Loop";
    case "loop-next":
      return "Loop next";
    case "terminal-return":
      return "Return";
    case "terminal-revert":
      return "Revert";
    case "control-transfer":
      return "Control transfer";
    case "assembly":
      return "Assembly";
    case "try":
      return "Try";
    case "catch":
      return "Catch";
    case "block":
      return "Block";
    // biome-ignore lint/complexity/noUselessSwitchCase: Keep the supported fallback kind explicit while safely handling unknown server payloads.
    case "statement":
    default:
      return "Statement";
  }
}

function edgeKindLabel(kind?: GraphEdgeKind): string {
  switch (kind) {
    case "imports":
      return "imports";
    case "inherits":
      return "inherits from";
    case "precedes":
      return "precedes";
    case "branch-true":
      return "true branch to";
    case "branch-false":
      return "false branch to";
    case "loop-back":
      return "loops back to";
    case "return":
      return "returns to";
    case "revert":
      return "reverts to";
    case "break":
      return "breaks to";
    case "continue":
      return "continues to";
    // biome-ignore lint/complexity/noUselessSwitchCase: Keep the supported fallback kind explicit while safely handling unknown server payloads.
    case "normal":
    default:
      return "flows to";
  }
}

function edgeRelationshipText(
  edge: GraphEdgeLike,
  nodeLabels: Map<string, string>
): string {
  const from = nodeLabels.get(edge.from) ?? edge.from;
  const to = nodeLabels.get(edge.to) ?? edge.to;
  const label = edge.label?.trim();
  const kind = edge.kind ? edgeKindLabel(edge.kind) : label || edgeKindLabel();
  const normalizedLabel = label?.toLowerCase();
  const distinctLabel =
    edge.kind &&
    label &&
    normalizedLabel !== kind.toLowerCase() &&
    normalizedLabel !== edge.kind
      ? `, labeled ${label}`
      : "";
  return `${from} ${kind} ${to}${distinctLabel}.`;
}

function renderAccessibleGraphDetails(graph: GraphDocumentLike): string {
  const nodeLabels = new Map(graph.nodes.map((node) => [node.id, node.label]));
  const nodeItems = graph.nodes
    .map(
      (node) => `<li>
        <span class="node-kind-cue">${escapeHtml(nodeKindLabel(node.kind))}</span><strong>${escapeHtml(node.label)}</strong>
        <span class="node-detail">${escapeHtml(node.detail)}</span>
      </li>`
    )
    .join("");
  const edgeItems = graph.edges
    .map(
      (edge) =>
        `<li>${escapeHtml(edgeRelationshipText(edge, nodeLabels))}</li>`
    )
    .join("");

  const sources = new Map<string, Set<string>>();
  for (const node of graph.nodes) {
    if (!node.uri) {
      continue;
    }
    const labels = sources.get(node.uri) ?? new Set<string>();
    labels.add(node.label);
    sources.set(node.uri, labels);
  }
  const sourceItems = Array.from(sources, ([uri, nodeLabelSet]) => {
    const labels = summarizeSourceNodeLabels(nodeLabelSet);
    const label = sourceLabel(uri);
    return `<li>
      <button class="source-button" type="button" aria-label="${escapeHtmlAttribute(
        `Open source ${label} for ${labels}`
      )}" data-source-uri="${escapeHtmlAttribute(uri)}">Open ${escapeHtml(
        label
      )}</button>
      <span class="node-detail">Source for ${escapeHtml(labels)}</span>
    </li>`;
  }).join("");

  return `<details class="graph-details">
    <summary>Graph details and source links</summary>
    <div class="graph-details-content">
      <section class="graph-details-section" aria-labelledby="graph-node-details-heading">
        <h2 id="graph-node-details-heading">Nodes</h2>
        ${nodeItems ? `<ul>${nodeItems}</ul>` : "<p>No nodes.</p>"}
      </section>
      <section class="graph-details-section" aria-labelledby="graph-edge-details-heading">
        <h2 id="graph-edge-details-heading">Relationships</h2>
        ${edgeItems ? `<ul>${edgeItems}</ul>` : "<p>No relationships.</p>"}
      </section>
      ${
        sourceItems
          ? `<section class="graph-details-section" aria-labelledby="graph-source-details-heading">
        <h2 id="graph-source-details-heading">Source links</h2>
        <ul>${sourceItems}</ul>
      </section>`
          : ""
      }
    </div>
  </details>`;
}

function summarizeSourceNodeLabels(labels: ReadonlySet<string>): string {
  const allLabels = Array.from(labels);
  const visibleLabels = allLabels
    .slice(0, SOURCE_LABEL_SUMMARY_LIMIT)
    .map((label) => truncateText(label, SOURCE_LABEL_MAX_CHARS));
  const remaining = allLabels.length - visibleLabels.length;
  return `${visibleLabels.join(", ")}${remaining > 0 ? `, and ${remaining} more` : ""}`;
}

function assessDisplayedGraphRenderBudget(
  graph: GraphDocumentLike
): GraphRenderBudget {
  let textLength = Math.min(
    GRAPH_RENDER_TEXT_LIMIT + 1,
    graph.title.length
  );
  const addText = (value: string | undefined): void => {
    if (!value || textLength > GRAPH_RENDER_TEXT_LIMIT) {
      return;
    }
    textLength = Math.min(
      GRAPH_RENDER_TEXT_LIMIT + 1,
      textLength + value.length
    );
  };

  for (const node of graph.nodes) {
    addText(node.id);
    addText(node.label);
    addText(node.detail);
    addText(node.uri);
  }
  for (const edge of graph.edges) {
    addText(edge.from);
    addText(edge.to);
    addText(edge.label);
  }

  const nodeCount = graph.nodes.length;
  const edgeCount = graph.edges.length;
  return {
    canRender:
      nodeCount <= GRAPH_RENDER_NODE_LIMIT &&
      edgeCount <= GRAPH_RENDER_EDGE_LIMIT &&
      textLength <= GRAPH_RENDER_TEXT_LIMIT,
    edgeCount,
    edgeLimit: GRAPH_RENDER_EDGE_LIMIT,
    nodeCount,
    nodeLimit: GRAPH_RENDER_NODE_LIMIT,
    textLength,
    textLimit: GRAPH_RENDER_TEXT_LIMIT,
  };
}

function graphForDisplay(graph: GraphDocumentLike): GraphDocumentLike {
  if (graph.kind !== "control-flow") {
    return graph;
  }

  const nodeMap = new Map(graph.nodes.map((node) => [node.id, node]));
  const edges = graph.edges.filter(
    (edge) => !isSyntheticTerminalExitEdge(edge, nodeMap)
  );
  const connectedNodeIds = new Set<string>();
  for (const edge of edges) {
    connectedNodeIds.add(edge.from);
    connectedNodeIds.add(edge.to);
  }

  const nodes = graph.nodes.filter(
    (node) =>
      !(
        node.kind === "exit" &&
        graph.nodes.length > 1 &&
        !connectedNodeIds.has(node.id)
      )
  );

  if (nodes.length === graph.nodes.length && edges.length === graph.edges.length) {
    return graph;
  }

  const visibleNodeIds = new Set(nodes.map((node) => node.id));
  return {
    ...graph,
    edges: edges.filter(
      (edge) => visibleNodeIds.has(edge.from) && visibleNodeIds.has(edge.to)
    ),
    focusNodeId:
      graph.focusNodeId && visibleNodeIds.has(graph.focusNodeId)
        ? graph.focusNodeId
        : nodes[0]?.id,
    nodes,
  };
}

function isSyntheticTerminalExitEdge(
  edge: GraphEdgeLike,
  nodeMap: Map<string, GraphNodeLike>
): boolean {
  const from = nodeMap.get(edge.from);
  const to = nodeMap.get(edge.to);
  return (
    to?.kind === "exit" &&
    (from?.kind === "terminal-return" ||
      from?.kind === "terminal-revert" ||
      from?.kind === "control-transfer") &&
    (edge.kind === "return" || edge.kind === "revert")
  );
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
    return `${graph.nodes.length} ${nodeLabel}, ${graph.edges.length} ${edgeLabel}. Function-level CFG${
      semanticSummary ? `, including ${semanticSummary}` : ""
    }.`;
  }
  return `${graph.nodes.length} ${nodeLabel}, ${graph.edges.length} ${edgeLabel}`;
}

function isBackEdge(kind?: GraphEdgeKind): boolean {
  return kind === "loop-back" || kind === "break" || kind === "continue";
}

function layoutGraph(graph: GraphDocumentLike): LayoutGraph {
  const direction =
    graph.kind === "control-flow" || graph.kind === "linearized-inheritance"
      ? "TD"
      : "LR";
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

    const width = Math.max(x - 88 + H_PADDING, H_PADDING * 2);
    const edges = graph.edges
      .map((edge) => layoutEdge(edge, nodeMap, direction))
      .filter((edge): edge is LayoutEdge => edge !== undefined);
    return layoutWithEdgeExtents(
      direction,
      nodes,
      edges,
      width,
      Math.max(height, V_PADDING * 2)
    );
  }

  const { height, width } = placeTopDownNodes(
    graph,
    nodes,
    levels,
    levelBuckets,
    sortedLevels
  );
  const edges = graph.edges
    .map((edge) => layoutEdge(edge, nodeMap, direction))
    .filter((edge): edge is LayoutEdge => edge !== undefined);
  return layoutWithEdgeExtents(direction, nodes, edges, width, height);
}

function layoutWithEdgeExtents(
  direction: "LR" | "TD",
  nodes: LayoutNode[],
  edges: LayoutEdge[],
  baseWidth: number,
  baseHeight: number
): LayoutGraph {
  const viewBoxX = Math.min(0, ...edges.map((edge) => edge.minX));
  const viewBoxY = Math.min(0, ...edges.map((edge) => edge.minY));
  const maxX = Math.max(baseWidth, ...edges.map((edge) => edge.maxX));
  const maxY = Math.max(baseHeight, ...edges.map((edge) => edge.maxY));
  return {
    direction,
    edges,
    height: Math.max(1, maxY - viewBoxY),
    nodes,
    viewBoxX,
    viewBoxY,
    width: Math.max(1, maxX - viewBoxX),
  };
}

function placeTopDownNodes(
  graph: GraphDocumentLike,
  nodes: LayoutNode[],
  levels: Map<string, number>,
  levelBuckets: Map<number, LayoutNode[]>,
  sortedLevels: number[]
): { height: number; width: number } {
  const nodeById = new Map(nodes.map((node) => [node.id, node]));
  const centers = new Map<string, number>();

  for (const level of sortedLevels) {
    const bucket = levelBuckets.get(level) ?? [];
    if (bucket.length === 0) {
      continue;
    }

    const ranked = bucket
      .map((node, index) => ({
        index,
        node,
        preferredCenter: preferredNodeCenter(
          graph,
          node,
          levels,
          centers,
          nodeById
        ),
      }))
      .sort((a, b) => {
        if (a.preferredCenter === b.preferredCenter) {
          return a.index - b.index;
        }
        return a.preferredCenter - b.preferredCenter;
      });

    let rightEdge = Number.NEGATIVE_INFINITY;
    for (const item of ranked) {
      const leftmostCenter =
        rightEdge === Number.NEGATIVE_INFINITY
          ? item.preferredCenter
          : rightEdge + TD_NODE_GAP + item.node.width / 2;
      const center = Math.max(item.preferredCenter, leftmostCenter);
      centers.set(item.node.id, center);
      rightEdge = center + item.node.width / 2;
    }

    const rowMin = Math.min(
      ...ranked.map((item) => (centers.get(item.node.id) ?? 0) - item.node.width / 2)
    );
    const rowMax = Math.max(
      ...ranked.map((item) => (centers.get(item.node.id) ?? 0) + item.node.width / 2)
    );
    const preferredAverage =
      ranked.reduce((sum, item) => sum + item.preferredCenter, 0) / ranked.length;
    const rowShift = preferredAverage - (rowMin + rowMax) / 2;
    for (const item of ranked) {
      centers.set(item.node.id, (centers.get(item.node.id) ?? 0) + rowShift);
    }
  }

  let y = V_PADDING;
  let minX = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  for (const level of sortedLevels) {
    const bucket = levelBuckets.get(level) ?? [];
    const maxHeight = Math.max(...bucket.map((node) => node.height), 100);
    for (const node of bucket) {
      const center = centers.get(node.id) ?? 0;
      node.x = center - node.width / 2;
      node.y = y;
      minX = Math.min(minX, node.x);
      maxX = Math.max(maxX, node.x + node.width);
    }
    y += maxHeight + TD_LEVEL_GAP;
  }

  if (!Number.isFinite(minX) || !Number.isFinite(maxX)) {
    return { height: V_PADDING * 2, width: H_PADDING * 2 };
  }

  const shiftX = H_PADDING - minX;
  for (const node of nodes) {
    node.x += shiftX;
  }

  return {
    height: Math.max(y - TD_LEVEL_GAP + V_PADDING, V_PADDING * 2),
    width: maxX - minX + H_PADDING * 2,
  };
}

function preferredNodeCenter(
  graph: GraphDocumentLike,
  node: LayoutNode,
  levels: Map<string, number>,
  centers: Map<string, number>,
  nodeById: Map<string, LayoutNode>
): number {
  const incoming = graph.edges.filter((edge) => isForwardIncoming(edge, node, levels));
  if (incoming.length === 0) {
    return fallbackCenterForUnplacedNode(centers);
  }

  const parentCenters = incoming
    .map((edge) => {
      const parentCenter = centers.get(edge.from);
      return parentCenter === undefined
        ? undefined
        : parentCenter +
            successorLaneOffset(graph, edge, levels, nodeById);
    })
    .filter((value): value is number => value !== undefined);
  if (parentCenters.length === 0) {
    return fallbackCenterForUnplacedNode(centers);
  }
  return parentCenters.reduce((sum, value) => sum + value, 0) / parentCenters.length;
}

function fallbackCenterForUnplacedNode(centers: Map<string, number>): number {
  if (centers.size === 0) {
    return 0;
  }
  return Math.max(...centers.values()) + NODE_WIDTH_MIN + TD_NODE_GAP;
}

function isForwardIncoming(
  edge: GraphEdgeLike,
  node: LayoutNode,
  levels: Map<string, number>
): boolean {
  if (edge.to !== node.id || isBackEdge(edge.kind)) {
    return false;
  }
  const fromLevel = levels.get(edge.from);
  const toLevel = levels.get(edge.to);
  return fromLevel !== undefined && toLevel !== undefined && fromLevel < toLevel;
}

function successorLaneOffset(
  graph: GraphDocumentLike,
  edge: GraphEdgeLike,
  levels: Map<string, number>,
  nodeById: Map<string, LayoutNode>
): number {
  const fromLevel = levels.get(edge.from);
  if (fromLevel === undefined) {
    return 0;
  }
  const successors = graph.edges
    .filter((candidate) => {
      const toLevel = levels.get(candidate.to);
      return (
        candidate.from === edge.from &&
        !isBackEdge(candidate.kind) &&
        toLevel !== undefined &&
        fromLevel < toLevel
      );
    })
    .sort(compareOutgoingEdges);
  if (successors.length <= 1) {
    return 0;
  }

  const index = successors.findIndex(
    (candidate) =>
      candidate.to === edge.to &&
      candidate.kind === edge.kind &&
      candidate.label === edge.label
  );
  if (index < 0) {
    return 0;
  }
  const spread = Math.max(
    NODE_WIDTH_MIN + TD_BRANCH_LANE_GAP,
    ...successors.map(
      (candidate) =>
        (nodeById.get(candidate.to)?.width ?? NODE_WIDTH_MIN) + TD_BRANCH_LANE_GAP
    )
  );
  return (index - (successors.length - 1) / 2) * spread;
}

function compareOutgoingEdges(a: GraphEdgeLike, b: GraphEdgeLike): number {
  const orderA = outgoingEdgeOrder(a.kind);
  const orderB = outgoingEdgeOrder(b.kind);
  if (orderA !== orderB) {
    return orderA - orderB;
  }
  return `${a.label ?? ""}:${a.to}`.localeCompare(`${b.label ?? ""}:${b.to}`);
}

function outgoingEdgeOrder(kind?: GraphEdgeKind): number {
  switch (kind) {
    case "branch-true":
      return 0;
    case "normal":
      return 1;
    case "branch-false":
      return 2;
    default:
      return 3;
  }
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
    const labelPoint = cubicPoint(
      labelT(edge.kind),
      { x: startX, y: startY },
      { x: startX + control, y: startY },
      { x: endX - control, y: endY },
      { x: endX, y: endY }
    );
    const points = [
      { x: startX, y: startY },
      { x: startX + control, y: startY },
      { x: endX - control, y: endY },
      { x: endX, y: endY },
    ];
    return edgeLayoutWithBounds(
      edge,
      labelPoint,
      `M ${startX} ${startY} C ${startX + control} ${startY}, ${endX - control} ${endY}, ${endX} ${endY}`,
      points
    );
  }

  if (Math.abs(from.y - to.y) < 1) {
    const fromCenterX = from.x + from.width / 2;
    const toCenterX = to.x + to.width / 2;
    const flowsRight = fromCenterX <= toCenterX;
    const startX = flowsRight ? from.x + from.width : from.x;
    const endX = flowsRight
      ? to.x - SAME_RANK_EDGE_GAP
      : to.x + to.width + SAME_RANK_EDGE_GAP;
    const startY = from.y + from.height / 2;
    const endY = to.y + to.height / 2;
    const control = Math.min(48, Math.max(1, Math.abs(endX - startX) * 0.45));
    const firstControlX = flowsRight
      ? startX + control
      : startX - control;
    const secondControlX = flowsRight
      ? endX - control
      : endX + control;
    const labelPoint = cubicPoint(
      labelT(edge.kind),
      { x: startX, y: startY },
      { x: firstControlX, y: startY },
      { x: secondControlX, y: endY },
      { x: endX, y: endY }
    );
    const points = [
      { x: startX, y: startY },
      { x: firstControlX, y: startY },
      { x: secondControlX, y: endY },
      { x: endX, y: endY },
    ];
    return edgeLayoutWithBounds(
      edge,
      labelPoint,
      flowsRight
        ? `M ${startX} ${startY} C ${startX + control} ${startY}, ${endX - control} ${endY}, ${endX} ${endY}`
        : `M ${startX} ${startY} C ${startX - control} ${startY}, ${endX + control} ${endY}, ${endX} ${endY}`,
      points
    );
  }

  const startX = from.x + from.width / 2;
  const startY = from.y + from.height;
  const endX = to.x + to.width / 2;
  const endY = to.y;
  const control = Math.max(48, Math.abs(endY - startY) * 0.35);
  const labelPoint = cubicPoint(
    labelT(edge.kind),
    { x: startX, y: startY },
    { x: startX, y: startY + control },
    { x: endX, y: endY - control },
    { x: endX, y: endY }
  );
  const points = [
    { x: startX, y: startY },
    { x: startX, y: startY + control },
    { x: endX, y: endY - control },
    { x: endX, y: endY },
  ];
  return edgeLayoutWithBounds(
    edge,
    labelPoint,
    `M ${startX} ${startY} C ${startX} ${startY + control}, ${endX} ${endY - control}, ${endX} ${endY}`,
    points
  );
}

function edgeLayoutWithBounds(
  edge: GraphEdgeLike,
  labelPoint: { x: number; y: number },
  path: string,
  routePoints: { x: number; y: number }[]
): LayoutEdge {
  const halfLabelWidth = EDGE_LABEL_WIDTH_MAX / 2;
  const routeMinX = Math.min(
    ...routePoints.map((point) => point.x),
    labelPoint.x - halfLabelWidth
  );
  const routeMaxX = Math.max(
    ...routePoints.map((point) => point.x),
    labelPoint.x + halfLabelWidth
  );
  const routeMinY = Math.min(
    ...routePoints.map((point) => point.y),
    labelPoint.y - 10
  );
  const routeMaxY = Math.max(
    ...routePoints.map((point) => point.y),
    labelPoint.y + 10
  );
  return {
    ...edge,
    labelX: labelPoint.x,
    labelY: labelPoint.y,
    maxX: routeMaxX + EDGE_ROUTE_MARGIN,
    maxY: routeMaxY + EDGE_ROUTE_MARGIN,
    minX: routeMinX - EDGE_ROUTE_MARGIN,
    minY: routeMinY - EDGE_ROUTE_MARGIN,
    path,
  };
}

function labelT(kind?: GraphEdgeKind): number {
  return kind === "branch-true" || kind === "branch-false" ? 0.38 : 0.5;
}

function cubicPoint(
  t: number,
  start: { x: number; y: number },
  controlA: { x: number; y: number },
  controlB: { x: number; y: number },
  end: { x: number; y: number }
): { x: number; y: number } {
  const inverse = 1 - t;
  const startWeight = inverse ** 3;
  const controlAWeight = 3 * inverse ** 2 * t;
  const controlBWeight = 3 * inverse * t ** 2;
  const endWeight = t ** 3;
  return {
    x:
      startWeight * start.x +
      controlAWeight * controlA.x +
      controlBWeight * controlB.x +
      endWeight * end.x,
    y:
      startWeight * start.y +
      controlAWeight * controlA.y +
      controlBWeight * controlB.y +
      endWeight * end.y,
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
  const labelLines = nodeLabelLines(node);
  const detailLines = nodeDetailLines(node);
  const actionHeight = node.uri
    ? NODE_SOURCE_ACTION_GAP + NODE_SOURCE_ACTION_HEIGHT
    : 0;
  return Math.max(
    68,
    26 + labelLines.length * 16 + 14 + detailLines.length * 14 + 12 + actionHeight
  );
}

function measureNodeWidth(node: GraphNodeLike): number {
  const longestLine = Math.max(
    ...nodeLabelLines(node).map((line) => line.length),
    ...nodeDetailLines(node).map((line) => line.length),
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

function pluralize(count: number, singular: string): string {
  return count === 1 ? singular : `${singular}s`;
}

function nodeLabelLines(node: GraphNodeLike): string[] {
  return limitLines(
    wrapText(node.label, NODE_LABEL_WRAP),
    NODE_LABEL_LINES_MAX,
    NODE_LABEL_WRAP
  );
}

function nodeDetailLines(node: GraphNodeLike): string[] {
  return limitLines(
    wrapText(node.detail, NODE_DETAIL_WRAP),
    NODE_DETAIL_LINES_MAX,
    NODE_DETAIL_WRAP
  );
}

function limitLines(
  lines: string[],
  maxLines: number,
  maxChars: number
): string[] {
  if (lines.length <= maxLines) {
    return lines;
  }

  const limited = lines.slice(0, maxLines);
  const finalIndex = limited.length - 1;
  const suffix = "...";
  limited[finalIndex] = `${limited[finalIndex].slice(
    0,
    Math.max(0, maxChars - suffix.length)
  )}${suffix}`;
  return limited;
}

function truncateText(value: string, maxChars: number): string {
  const characters = Array.from(value);
  if (characters.length <= maxChars) {
    return value;
  }
  return `${characters.slice(0, Math.max(0, maxChars - 1)).join("")}…`;
}

function renderNodeText(node: LayoutNode): string {
  const labelLines = nodeLabelLines(node);
  const detailLines = nodeDetailLines(node);
  const lines: string[] = [];
  let y = 28;

  for (const line of labelLines) {
    lines.push(
      `<text class="node-label" x="16" y="${y}">${escapeHtml(line)}</text>`
    );
    y += 16;
  }

  lines.push(
    `<text class="node-kind" x="16" y="${y}">${escapeHtml(
      nodeKindLabel(node.kind)
    )}</text>`
  );
  y += 14;

  for (const line of detailLines) {
    lines.push(
      `<text class="node-meta" x="16" y="${y}">${escapeHtml(line)}</text>`
    );
    y += 14;
  }

  return lines.join("");
}

function renderNodeSourceAction(node: LayoutNode): string {
  if (!node.uri) {
    return "";
  }

  const y = node.height - NODE_SOURCE_ACTION_HEIGHT - 12;
  return `<g class="source-chip" data-source-uri="${escapeHtmlAttribute(node.uri)}">
    <title>${escapeHtml(`Open source ${sourceLabel(node.uri)}`)}</title>
    <rect x="16" y="${y}" width="${NODE_SOURCE_ACTION_WIDTH}" height="${NODE_SOURCE_ACTION_HEIGHT}" rx="6" ry="6"></rect>
    <text x="${16 + NODE_SOURCE_ACTION_WIDTH / 2}" y="${y + 15}" text-anchor="middle">Open source</text>
  </g>`;
}

function renderEdgeLabel(edge: LayoutEdge, label: string): string {
  const displayLabel = truncateText(label, EDGE_LABEL_MAX_CHARS);
  const width = clamp(displayLabel.length * 7 + 18, 32, EDGE_LABEL_WIDTH_MAX);
  const height = 20;
  return `<g class="edge-label-pill ${edgeClass(edge.kind)}" transform="translate(${edge.labelX} ${edge.labelY})">
    <title>${escapeHtml(label)}</title>
    <rect class="edge-label-box" x="${-width / 2}" y="${-height / 2}" width="${width}" height="${height}" rx="10" ry="10"></rect>
    <text class="edge-label" x="0" y="0">${escapeHtml(displayLabel)}</text>
  </g>`;
}

function renderSvg(graph: GraphDocumentLike, layout: LayoutGraph): string {
  const nodeLabels = new Map(graph.nodes.map((node) => [node.id, node.label]));
  return `<svg width="${layout.width}" height="${layout.height}" viewBox="${layout.viewBoxX} ${layout.viewBoxY} ${layout.width} ${layout.height}" aria-hidden="true" focusable="false">
    <defs>
      <marker id="arrow-default" markerWidth="10" markerHeight="10" refX="8" refY="5" orient="auto" markerUnits="strokeWidth">
        <path class="arrow-head" d="M 0 0 L 10 5 L 0 10 z" />
      </marker>
      <marker id="arrow-branch-true" markerWidth="10" markerHeight="10" refX="8" refY="5" orient="auto" markerUnits="strokeWidth">
        <path class="arrow-head branch-true" d="M 0 0 L 10 5 L 0 10 z" />
      </marker>
      <marker id="arrow-branch-false" markerWidth="10" markerHeight="10" refX="8" refY="5" orient="auto" markerUnits="strokeWidth">
        <path class="arrow-head branch-false" d="M 0 0 L 10 5 L 0 10 z" />
      </marker>
    </defs>
    ${renderBackdrop(layout)}
    ${layout.edges
      .map((edge) => {
        const label = edgeLabel(graph, edge);
        return `<g>
          <title>${escapeHtml(edgeRelationshipText(edge, nodeLabels))}</title>
          <path class="edge ${edgeClass(edge.kind)}" d="${edge.path}" marker-end="url(#${edgeMarkerId(edge.kind)})"></path>
          ${label ? renderEdgeLabel(edge, label) : ""}
        </g>`;
      })
      .join("")}
    ${layout.nodes
      .map((node) => {
        const visualClass = nodeVisualClass(node.kind);
        const focusClass = graph.focusNodeId === node.id ? " focus" : "";
        return `<g class="node-card-svg ${visualClass}${focusClass}" transform="translate(${node.x} ${node.y})">
          <title>${escapeHtml(`${nodeKindLabel(node.kind)}: ${node.label}. ${node.detail}`)}</title>
          <rect rx="${NODE_RADIUS}" ry="${NODE_RADIUS}" width="${node.width}" height="${node.height}"></rect>
          ${renderNodeText(node)}
          ${renderNodeSourceAction(node)}
        </g>`;
      })
      .join("")}
  </svg>`;
}

function edgeLabel(
  graph: GraphDocumentLike,
  edge: LayoutEdge
): string | undefined {
  if (!edge.label) {
    return undefined;
  }
  if (
    graph.kind !== "control-flow" &&
    (edge.label === edge.kind ||
      edge.label === "imports" ||
      edge.label === "inherits" ||
      edge.label === "precedes")
  ) {
    return undefined;
  }
  if (
    graph.kind === "control-flow" &&
    (edge.kind === "return" || edge.kind === "revert")
  ) {
    return undefined;
  }
  return edge.label;
}

function renderBackdrop(layout: LayoutGraph): string {
  if (layout.direction === "LR") {
    const columns: string[] = [];
    for (
      let x = layout.viewBoxX + H_PADDING;
      x < layout.viewBoxX + layout.width - H_PADDING;
      x += 120
    ) {
      columns.push(
        `<line class="grid-line" x1="${x}" y1="${layout.viewBoxY + V_PADDING / 2}" x2="${x}" y2="${layout.viewBoxY + layout.height - V_PADDING / 2}"></line>`
      );
    }
    return columns.join("");
  }

  const rows: string[] = [];
  for (
    let y = layout.viewBoxY + V_PADDING;
    y < layout.viewBoxY + layout.height - V_PADDING;
    y += 120
  ) {
    rows.push(
      `<line class="grid-line" x1="${layout.viewBoxX + H_PADDING / 2}" y1="${y}" x2="${layout.viewBoxX + layout.width - H_PADDING / 2}" y2="${y}"></line>`
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
  const words = value
    .split(/\s+/)
    .filter(Boolean)
    .flatMap((word) => splitLongWord(word, maxChars));
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

function splitLongWord(word: string, maxChars: number): string[] {
  if (word.length <= maxChars) {
    return [word];
  }

  const parts: string[] = [];
  let remaining = word;
  while (remaining.length > maxChars) {
    const preferredBreak = Math.max(
      remaining.lastIndexOf("/", maxChars),
      remaining.lastIndexOf("-", maxChars),
      remaining.lastIndexOf("_", maxChars)
    );
    const splitAt =
      preferredBreak >= Math.floor(maxChars * 0.55)
        ? preferredBreak + 1
        : maxChars;
    parts.push(remaining.slice(0, splitAt));
    remaining = remaining.slice(splitAt);
  }
  if (remaining) {
    parts.push(remaining);
  }
  return parts;
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
