import { describe, expect, it, vi } from "vitest";

const vscodeHarness = vi.hoisted(() => {
  let disposeHandler: (() => void) | undefined;
  const webview = {
    cspSource: "vscode-webview://test",
    html: "",
    onDidReceiveMessage: vi.fn(() => ({ dispose: vi.fn() })),
  };
  const panel = {
    title: "",
    webview,
    reveal: vi.fn(),
    onDidDispose: vi.fn((handler: () => void) => {
      disposeHandler = handler;
      return { dispose: vi.fn() };
    }),
  };

  return {
    disposePanel: () => disposeHandler?.(),
    module: {
      ViewColumn: { Beside: 2 },
      Uri: { parse: vi.fn() },
      commands: { executeCommand: vi.fn() },
      window: {
        activeTextEditor: undefined,
        createWebviewPanel: vi.fn(() => panel),
        showErrorMessage: vi.fn(),
        showWarningMessage: vi.fn(),
      },
    },
    panel,
  };
});

vi.mock("vscode", () => vscodeHarness.module);

import {
  getGraphPreviewSnapshot,
  graphRequestKey,
  renderGraphStatusWebviewHtml,
  showGraph,
} from "./graphPreview";
import {
  assessGraphRenderBudget,
  buildGraphPreviewSnapshot,
  calculateGraphFitScale,
  clampManualGraphScale,
  GRAPH_RENDER_EDGE_LIMIT,
  GRAPH_RENDER_NODE_LIMIT,
  GRAPH_RENDER_TEXT_LIMIT,
  isGraphDocumentLike,
  renderGraphWebviewHtml,
} from "./graphPreviewRender";

describe("graph preview request state", () => {
  it("builds stable identities for distinct graph requests", () => {
    const imports = graphRequestKey({
      kind: "imports",
      uri: "file:///workspace/Vault.sol",
    });
    const controlFlow = graphRequestKey({
      kind: "control-flow",
      uri: "file:///workspace/Vault.sol",
      symbolName: "run",
      targetOffset: 42,
    });

    expect(imports).toBe(
      '["imports","file:///workspace/Vault.sol",null,null]'
    );
    expect(controlFlow).not.toBe(imports);
  });

  it("renders accessible, CSP-protected loading and error states", () => {
    const loading = renderGraphStatusWebviewHtml(
      {
        kind: "loading",
        title: "Building an imports graph",
        message: "Waiting for solgrid.",
      },
      { cspSource: "vscode-webview://test", nonce: "safe-nonce" }
    );
    const error = renderGraphStatusWebviewHtml(
      {
        kind: "error",
        title: "Could not build <graph>",
        message: 'Server said: <script>alert("x")</script>',
      },
      { cspSource: "vscode-webview://test", nonce: "safe-nonce" }
    );

    expect(loading).toContain("role=\"status\"");
    expect(loading).toContain("aria-live=\"polite\"");
    expect(loading).toContain("&#39;nonce-safe-nonce&#39;");
    expect(loading).toContain('<style nonce="safe-nonce">');
    expect(loading).toContain("prefers-reduced-motion: reduce");
    expect(loading).toContain(
      "body.vscode-light,\n      body.vscode-high-contrast-light { color-scheme: light; }"
    );
    expect(loading).toContain("body.vscode-high-contrast main");
    expect(error).toContain("role=\"alert\"");
    expect(error).toContain("aria-live=\"assertive\"");
    expect(error).toContain("Could not build &lt;graph&gt;");
    expect(error).toContain(
      "Server said: &lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;"
    );
    expect(error).not.toContain("<script>alert");
  });

  it("clears stale snapshots while loading and when the panel closes", async () => {
    const firstGraph = {
      kind: "imports" as const,
      title: "Imports graph for Vault.sol",
      focusNodeId: "vault",
      nodes: [{ id: "vault", label: "Vault.sol", detail: "Source file" }],
      edges: [],
    };
    const secondGraph = {
      ...firstGraph,
      title: "Imports graph for Main.sol",
      focusNodeId: "main",
      nodes: [{ id: "main", label: "Main.sol", detail: "Source file" }],
    };
    const firstClient = {
      sendRequest: vi.fn().mockResolvedValue(firstGraph),
    };

    await showGraph(firstClient as never, {
      kind: "imports",
      uri: "file:///workspace/Vault.sol",
    });
    const firstSnapshot = getGraphPreviewSnapshot();
    expect(firstSnapshot?.title).toBe("Imports graph for Vault.sol");

    let resolveSecond: ((graph: typeof secondGraph) => void) | undefined;
    const secondClient = {
      sendRequest: vi.fn(
        () =>
          new Promise<typeof secondGraph>((resolve) => {
            resolveSecond = resolve;
          })
      ),
    };
    const pending = showGraph(secondClient as never, {
      kind: "imports",
      uri: "file:///workspace/Main.sol",
    });

    expect(getGraphPreviewSnapshot()).toBeUndefined();
    expect(vscodeHarness.panel.webview.html).toContain(
      "Waiting for the solgrid language server"
    );

    resolveSecond?.(secondGraph);
    await pending;
    const secondSnapshot = getGraphPreviewSnapshot();
    expect(secondSnapshot?.title).toBe("Imports graph for Main.sol");
    expect(secondSnapshot?.requestId).toBeGreaterThan(
      firstSnapshot?.requestId ?? 0
    );
    expect(secondSnapshot?.requestKey).toContain("Main.sol");

    vscodeHarness.disposePanel();
    expect(getGraphPreviewSnapshot()).toBeUndefined();
  });

  it("refuses oversized graphs explicitly without rendering a partial preview", async () => {
    const oversizedGraph = {
      kind: "imports" as const,
      title: "Oversized imports graph",
      nodes: Array.from({ length: GRAPH_RENDER_NODE_LIMIT + 1 }, (_, index) => ({
        id: `node-${index}`,
        label: `Node ${index}`,
        detail: "Source file",
      })),
      edges: [],
    };
    const client = {
      sendRequest: vi.fn().mockResolvedValue(oversizedGraph),
    };

    await showGraph(client as never, {
      kind: "imports",
      uri: "file:///workspace/Main.sol",
    });

    expect(getGraphPreviewSnapshot()).toBeUndefined();
    expect(vscodeHarness.panel.webview.html).toContain(
      "Graph too large to render"
    );
    expect(vscodeHarness.panel.webview.html).toContain(
      "not rendered or silently truncated"
    );
    expect(vscodeHarness.panel.webview.html).toContain("solgrid graph CLI");
    expect(vscodeHarness.panel.webview.html).not.toContain("<svg");
  });
});

describe("graph render budgets and scaling", () => {
  it("allows the documented thresholds and rejects every exceeded budget", () => {
    const atNodeLimit = {
      kind: "imports" as const,
      title: "At node limit",
      nodes: Array.from({ length: GRAPH_RENDER_NODE_LIMIT }, (_, index) => ({
        id: `node-${index}`,
        label: `Node ${index}`,
        detail: "Source file",
      })),
      edges: [],
    };
    expect(assessGraphRenderBudget(atNodeLimit).canRender).toBe(true);
    expect(
      assessGraphRenderBudget({
        ...atNodeLimit,
        nodes: [
          ...atNodeLimit.nodes,
          { id: "over-node-limit", label: "Over", detail: "Source file" },
        ],
      }).canRender
    ).toBe(false);

    const edgeNodes = [
      { id: "from", label: "From", detail: "Source file" },
      { id: "to", label: "To", detail: "Source file" },
    ];
    expect(
      assessGraphRenderBudget({
        kind: "imports",
        title: "Over edge limit",
        nodes: edgeNodes,
        edges: Array.from({ length: GRAPH_RENDER_EDGE_LIMIT + 1 }, () => ({
          from: "from",
          to: "to",
        })),
      }).canRender
    ).toBe(false);
    expect(
      assessGraphRenderBudget({
        kind: "imports",
        title: "Over text limit",
        nodes: [
          {
            id: "large",
            label: "x".repeat(GRAPH_RENDER_TEXT_LIMIT + 1),
            detail: "Source file",
          },
        ],
        edges: [],
      }).canRender
    ).toBe(false);
  });

  it("fits graphs far below the manual zoom floor without weakening that floor", () => {
    const fitted = calculateGraphFitScale(
      1_000,
      600,
      10_000,
      10_000,
      true
    );

    expect(fitted).toBeCloseTo(0.0588, 5);
    expect(10_000 * fitted).toBeLessThanOrEqual(600 - 12);
    expect(clampManualGraphScale(0.01)).toBe(0.2);
    expect(clampManualGraphScale(5)).toBe(1.8);
    expect(clampManualGraphScale(Number.NaN)).toBe(1);
  });
});

describe("renderGraphWebviewHtml", () => {
  it("renders an inheritance graph webview with svg content", () => {
    const graph = {
      kind: "inheritance" as const,
      title: "Inheritance graph for Vault",
      focusNodeId: "Vault",
      nodes: [
        {
          id: "Vault",
          label: "Vault",
          detail: "Contract in src/Vault.sol",
          kind: "contract" as const,
          uri: "file:///workspace/src/Vault.sol",
        },
        {
          id: "Ownable",
          label: "Ownable",
          detail: "Contract in lib/Ownable.sol",
          kind: "contract" as const,
          uri: "file:///workspace/lib/Ownable.sol",
        },
      ],
      edges: [
        {
          from: "Vault",
          to: "Ownable",
          label: "inherits",
          kind: "inherits" as const,
        },
      ],
    };

    const html = renderGraphWebviewHtml(graph, {
      cspSource: "vscode-webview://test",
      nonce: "nonce",
    });

    expect(html).toContain("<svg");
    expect(html).toContain("Inheritance graph for Vault");
    expect(html).toContain("Open source");
    expect(html).toContain("class=\"source-chip\"");
    expect(html).toContain("class=\"source-button\"");
    expect(html).toContain("data-source-uri=\"file:///workspace/src/Vault.sol\"");
    expect(html).toContain("Vault");
    expect(html).toContain("Ownable");
    expect(html).toContain("marker-end=\"url(#arrow-default)\"");
    expect(html).toContain('aria-hidden="true" focusable="false"');
    expect(html).not.toContain('<svg role="img"');
    expect(html).toContain(
      "--panel-bg: var(--vscode-editor-background);\n        --panel-bg: color-mix"
    );
    expect(html).toContain(
      "background: var(--vscode-editor-background);\n        background: color-mix"
    );
    expect(html).not.toContain("```mermaid");
  });

  it("renders linearization order without a node-list pane", () => {
    const html = renderGraphWebviewHtml(
      {
        kind: "linearized-inheritance",
        title: "Linearized inheritance for Vault",
        focusNodeId: "Vault",
        nodes: [
          { id: "Vault", label: "Vault", detail: "#1 Contract in src/Vault.sol" },
          {
            id: "AccessControl",
            label: "AccessControl",
            detail: "#2 Contract in src/AccessControl.sol",
          },
          { id: "Context", label: "Context", detail: "#3 Contract in src/Context.sol" },
        ],
        edges: [
          { from: "Vault", to: "AccessControl", label: "precedes" },
          { from: "AccessControl", to: "Context", label: "precedes" },
        ],
      },
      {
        cspSource: "vscode-webview://test",
        nonce: "nonce",
      }
    );

    expect(html).toContain("Order: Vault -&gt; AccessControl -&gt; Context");
    expect(html).toContain(
      'title="3 nodes, 2 edges. Order: Vault -&gt; AccessControl -&gt; Context"'
    );
    expect(html).toContain("-webkit-line-clamp: 3;");
    const graphWidth = Number(
      html.match(/data-graph-width="([\d.]+)"/)?.[1] ?? Number.NaN
    );
    expect(graphWidth).toBeLessThan(900);
    expect(html).not.toContain("node-list");
    expect(html).not.toContain("lineage-index");
  });

  it("wraps and clamps long unbroken titles in narrow panes", () => {
    const title = `Imports graph for ${"VeryLongUnbrokenContractName".repeat(8)}`;
    const html = renderGraphWebviewHtml(
      {
        kind: "imports",
        title,
        nodes: [],
        edges: [],
      },
      {
        cspSource: "vscode-webview://test",
        nonce: "nonce",
      }
    );

    expect(html).toContain(".title-block {\n        flex: 1 1 320px;\n        min-width: 0;");
    expect(html).toContain("overflow-wrap: anywhere;");
    expect(html).toContain("-webkit-line-clamp: 2;");
    expect(html).toContain(`<h1 title="${title}">${title}</h1>`);
  });

  it("renders a contained graph viewport with zoom controls", () => {
    const html = renderGraphWebviewHtml(
      {
        kind: "imports",
        title: "Imports graph for src/Vault.sol",
        focusNodeId: "src/Vault.sol",
        nodes: [
          {
            id: "src/Vault.sol",
            label: "src/Vault.sol",
            detail: "Source file",
            kind: "file",
          },
        ],
        edges: [],
      },
      {
        cspSource: "vscode-webview://test",
        nonce: "nonce",
      }
    );

    expect(html).toContain("height: 100vh;");
    expect(html).toContain("overflow: hidden;");
    expect(html).toContain("display: flex;");
    expect(html).toContain("data-graph-stage");
    expect(html).not.toMatch(/data-graph-height="[^"]+" style=/);
    expect(html).toContain("data-zoom-action=\"fit\"");
    expect(html).toContain(
      'class="canvas-wrap" tabindex="0" role="region" aria-label="Imports graph for src/Vault.sol interactive graph viewport"'
    );
    expect(html).toContain("canvasWrap.addEventListener(\"keydown\", scrollGraphViewport);");
    expect(html).toContain('case "ArrowLeft":');
    expect(html).toContain('case "PageDown":');
    expect(html).toContain('case "Home":');
    expect(html).toContain('case "End":');
    expect(html).toContain("canvasWrap.addEventListener(\"pointerdown\", beginPan);");
    expect(html).toContain("canvasWrap.addEventListener(\"dblclick\"");
    expect(html).toContain("\"wheel\"");
    expect(html).toContain("const WHEEL_ZOOM_SENSITIVITY = 0.002;");
    expect(html).toContain("const TRACKPAD_ZOOM_SENSITIVITY = 0.004;");
    expect(html).toContain("const zoomFactor = wheelZoomFactor(event);");
    expect(html).toContain("const calculateGraphFitScale =");
    expect(html).toContain("return calculateGraphFitScale(");
    expect(html).toContain("const clampManualGraphScale =");
    expect(html).toContain("setGraphScale(clampManualGraphScale(nextScale), options);");
    expect(html).toContain('? "<1%"');
    expect(html).toContain("setGraphScale(fitScale(), { preserveCenter: false });");
    expect(html).toContain('let autoFitMode = "width";');
    expect(html).toContain("autoFitMode = null;");
    expect(html).toContain("if (autoFitMode !== null)");
    expect(html).toContain('autoFitMode === "all"');
    expect(html).toContain("justify-content: center;");
    const graphWidth = Number(
      html.match(/data-graph-width="([\d.]+)"/)?.[1] ?? Number.NaN
    );
    expect(graphWidth).toBeLessThan(820);
    expect(html).toContain("typeof acquireVsCodeApi === \"function\"");
    expect(html).not.toContain("<aside");
  });

  it("renders a semantic graph alternative with deduplicated HTML source actions", () => {
    const html = renderGraphWebviewHtml(
      {
        kind: "inheritance",
        title: "Inheritance <graph>",
        nodes: [
          {
            id: "vault",
            label: "Vault <primary>",
            detail: "Owns & protects funds",
            kind: "contract",
            uri: "file:///workspace/Vault.sol",
          },
          {
            id: "alias",
            label: "Vault alias",
            detail: "The same source location",
            kind: "contract",
            uri: "file:///workspace/Vault.sol",
          },
        ],
        edges: [
          {
            from: "vault",
            to: "alias",
            kind: "inherits",
            label: "inherits",
          },
        ],
      },
      {
        cspSource: "vscode-webview://test",
        nonce: "nonce",
      }
    );

    expect(html).toContain("<summary>Graph details and source links</summary>");
    expect(html).toContain(
      'aria-label="Inheritance &lt;graph&gt; interactive graph viewport"'
    );
    expect(html).toContain('id="graph-node-details-heading">Nodes</h2>');
    expect(html).toContain('id="graph-edge-details-heading">Relationships</h2>');
    expect(html).toContain('<span class="node-kind-cue">Contract</span>');
    expect(html).toContain("Vault &lt;primary&gt;");
    expect(html).toContain("Owns &amp; protects funds");
    expect(html).toContain("Vault &lt;primary&gt; inherits from Vault alias");
    expect(html.match(/class="source-button"/g)).toHaveLength(1);
    expect(html).toContain("<button class=\"source-button\" type=\"button\"");
    expect(html).toContain(
      'aria-label="Open source Vault.sol for Vault &lt;primary&gt;, Vault alias"'
    );
    expect(html).toContain("Source for Vault &lt;primary&gt;, Vault alias");
    expect(html).not.toContain('class="source-chip" role="button"');
    expect(html).not.toContain('class="source-chip" tabindex="0"');
  });

  it("keeps shared-source focus announcements concise without dropping node semantics", () => {
    const html = renderGraphWebviewHtml(
      {
        kind: "inheritance",
        title: "Shared source",
        nodes: Array.from({ length: 6 }, (_, index) => ({
          id: `contract-${index + 1}`,
          label: `Contract ${index + 1}`,
          detail: `Contract ${index + 1} detail`,
          kind: "contract" as const,
          uri: "file:///workspace/Contracts.sol",
        })),
        edges: [],
      },
      {
        cspSource: "vscode-webview://test",
        nonce: "nonce",
      }
    );

    const sourceAction = html.match(
      /<button class="source-button"[\s\S]*?aria-label="([^"]+)"[\s\S]*?<span class="node-detail">([^<]+)<\/span>/
    );
    expect(sourceAction?.[1]).toContain(
      "Contract 1, Contract 2, Contract 3, and 3 more"
    );
    expect(sourceAction?.[2]).toContain(
      "Contract 1, Contract 2, Contract 3, and 3 more"
    );
    expect(sourceAction?.[1]).not.toContain("Contract 4");
    expect(html).toContain("<strong>Contract 4</strong>");
  });

  it("retains full node and edge content while safely truncating visual labels", () => {
    const html = renderGraphWebviewHtml(
      {
        kind: "control-flow",
        title: "Control flow",
        nodes: [
          {
            id: "entry",
            label: "A label that is deliberately much longer than the card",
            detail: "Detail that is also deliberately long and remains recoverable",
            kind: "entry",
          },
          { id: "exit", label: "Exit", detail: "Done", kind: "exit" },
        ],
        edges: [
          {
            from: "entry",
            to: "exit",
            kind: "branch-true",
            label: "a very long <unsafe> relationship label",
          },
        ],
      },
      {
        cspSource: "vscode-webview://test",
        nonce: "nonce",
      }
    );

    expect(html).toContain(
      "<title>Entry: A label that is deliberately much longer than the card. Detail that is also deliberately long and remains recoverable</title>"
    );
    expect(html).toContain(
      "<title>a very long &lt;unsafe&gt; relationship label</title>"
    );
    expect(html).toMatch(/<text class="edge-label"[^>]*>[^<]*…<\/text>/);
    expect(html).not.toContain(
      '<text class="edge-label" x="0" y="0">a very long &lt;unsafe&gt; relationship label</text>'
    );
    expect(html).toContain('<text class="node-kind"');
  });

  it("expands the viewBox to contain cyclic reverse-edge routes", () => {
    const html = renderGraphWebviewHtml(
      {
        kind: "control-flow",
        title: "Loop",
        focusNodeId: "entry",
        nodes: [
          { id: "entry", label: "Entry", detail: "Start", kind: "entry" },
          { id: "loop", label: "Loop", detail: "Again", kind: "loop" },
        ],
        edges: [
          { from: "loop", to: "entry", label: "again", kind: "loop-back" },
        ],
      },
      {
        cspSource: "vscode-webview://test",
        nonce: "nonce",
      }
    );

    const viewBox = html.match(
      /viewBox="(-?[\d.]+) (-?[\d.]+) ([\d.]+) ([\d.]+)"/
    );
    expect(viewBox).not.toBeNull();
    expect(Number(viewBox?.[2])).toBeLessThan(0);
    expect(Number(viewBox?.[4])).toBeGreaterThan(350);
  });

  it("includes focus-visible, high-contrast, and forced-color styles", () => {
    const html = renderGraphWebviewHtml(
      {
        kind: "imports",
        title: "Imports",
        nodes: [],
        edges: [],
      },
      {
        cspSource: "vscode-webview://test",
        nonce: "nonce",
      }
    );

    expect(html).toContain(".canvas-wrap:focus-visible");
    expect(html).toContain(".source-button:focus-visible");
    expect(html).toContain("body.vscode-light,\n      body.vscode-high-contrast-light");
    expect(html).toContain("body.vscode-dark,\n      body.vscode-high-contrast");
    expect(html).toContain(
      "--edge-default: var(--vscode-descriptionForeground);"
    );
    expect(html).toContain("body.vscode-high-contrast .node-card-svg > rect");
    expect(html).toContain(
      "body.vscode-high-contrast .node-card-svg.focus > rect"
    );
    expect(html).not.toContain(".node-card-svg.focus rect");
    expect(html).not.toContain(".node-card-svg.file rect");
    expect(html).not.toContain(".node-card-svg rect");
    expect(html.indexOf(".node-card-svg.focus > rect")).toBeGreaterThan(
      html.indexOf(".node-card-svg.structural > rect")
    );
    expect(html).toContain("stroke-dasharray: 7 3;");
    expect(html).toContain(
      "stroke: var(--vscode-focusBorder, var(--vscode-contrastActiveBorder, Highlight));"
    );
    expect(html).toContain("@media (forced-colors: active)");
  });

  it("hides synthetic terminal-to-exit edges in control-flow graphs", () => {
    const html = renderGraphWebviewHtml(
      {
        kind: "control-flow",
        title: "Control-flow graph for Vault.run",
        focusNodeId: "entry",
        nodes: [
          {
            id: "entry",
            label: "Entry",
            detail: "function run() public",
            kind: "entry",
          },
          {
            id: "branch",
            label: "if !authorized",
            detail: "if (!authorized) { revert Unauthorized(); }",
            kind: "branch",
          },
          {
            id: "revert",
            label: "revert",
            detail: "Unauthorized();",
            kind: "terminal-revert",
          },
          {
            id: "exit",
            label: "Exit",
            detail: "Flow leaves Vault.sol",
            kind: "exit",
          },
        ],
        edges: [
          { from: "entry", to: "branch", kind: "normal" },
          { from: "branch", to: "revert", label: "true", kind: "branch-true" },
          { from: "branch", to: "exit", label: "false", kind: "branch-false" },
          { from: "revert", to: "exit", label: "revert", kind: "revert" },
        ],
      },
      {
        cspSource: "vscode-webview://test",
        nonce: "nonce",
      }
    );

    expect(html).toContain("3 edges");
    expect(html).toContain("class=\"edge branch-true\"");
    expect(html).toContain('marker-end="url(#arrow-branch-true)"');
    expect(html).toContain("class=\"edge-label-pill branch-true\"");
    expect(html).toContain("class=\"edge branch-false\"");
    expect(html).toContain('marker-end="url(#arrow-branch-false)"');
    expect(html).toContain("class=\"edge-label-pill branch-false\"");
    expect(html).toContain(
      ".arrow-head.branch-true {\n        fill: var(--edge-true);"
    );
    expect(html).toContain(
      ".arrow-head.branch-false {\n        fill: var(--edge-false);"
    );
    expect(html).toContain("Exit");
    expect(html).toContain("revert");
    expect(html).not.toContain("edge revert");
    expect(html).not.toMatch(/<text class="edge-label"[^>]*>revert<\/text>/);
  });

  it("lays out control-flow branch successors in separate lanes", () => {
    const html = renderGraphWebviewHtml(
      {
        kind: "control-flow",
        title: "Control-flow graph for Vault.run",
        focusNodeId: "entry",
        nodes: [
          {
            id: "entry",
            label: "Entry",
            detail: "function run() public",
            kind: "entry",
          },
          {
            id: "branch",
            label: "if !authorized",
            detail: "if (!authorized) { revert Unauthorized(); }",
            kind: "branch",
          },
          {
            id: "revert",
            label: "revert",
            detail: "Unauthorized();",
            kind: "terminal-revert",
          },
          {
            id: "exit",
            label: "Exit",
            detail: "Flow leaves Vault.sol",
            kind: "exit",
          },
        ],
        edges: [
          { from: "entry", to: "branch", kind: "normal" },
          { from: "branch", to: "revert", label: "true", kind: "branch-true" },
          { from: "branch", to: "exit", label: "false", kind: "branch-false" },
          { from: "revert", to: "exit", label: "revert", kind: "revert" },
        ],
      },
      {
        cspSource: "vscode-webview://test",
        nonce: "nonce",
      }
    );

    const nodeTransforms = Array.from(
      html.matchAll(
        /class="node-card-svg [^"]*" transform="translate\(([-\d.]+) ([-\d.]+)\)"/g
      )
    ).map((match) => Number(match[1]));

    expect(nodeTransforms).toHaveLength(4);
    const branchX = nodeTransforms[1];
    const trueX = nodeTransforms[2];
    const falseX = nodeTransforms[3];
    expect(Math.abs(trueX - falseX)).toBeGreaterThan(100);
    expect(branchX).toBeGreaterThan(trueX);
    expect(branchX).toBeLessThan(falseX);
  });

  it("builds a stable preview snapshot for the test harness", () => {
    const snapshot = buildGraphPreviewSnapshot({
      kind: "control-flow",
      title: "Control-flow graph for Vault.run",
      focusNodeId: "entry",
      nodes: [
        {
          id: "entry",
          label: "Entry",
          detail: "function run(uint256 amount) public returns (uint256)",
          kind: "entry",
        },
        {
          id: "if",
          label: "if amount == 0",
          detail: "if (amount == 0) { return 1; }",
          kind: "branch",
        },
        {
          id: "exit",
          label: "Exit",
          detail: "Flow leaves Vault.sol",
          kind: "exit",
        },
      ],
      edges: [
        { from: "entry", to: "if", kind: "normal" },
        { from: "if", to: "exit", label: "return", kind: "return" },
      ],
    });

    expect(snapshot.title).toBe("Control-flow graph for Vault.run");
    expect(snapshot.kind).toBe("control-flow");
    expect(snapshot.focusLabel).toBe("Entry");
    expect(snapshot.nodeLabels).toEqual(["Entry", "if amount == 0", "Exit"]);
    expect(snapshot.edgeCount).toBe(2);
    expect(snapshot.summary).toContain("Function-level CFG");
  });

  it("omits orphan synthetic exits after terminal edge filtering", () => {
    const snapshot = buildGraphPreviewSnapshot({
      kind: "control-flow",
      title: "Control-flow graph for Vault.run",
      focusNodeId: "entry",
      nodes: [
        {
          id: "entry",
          label: "Entry",
          detail: "function run() public",
          kind: "entry",
        },
        {
          id: "revert",
          label: "revert",
          detail: "Unauthorized();",
          kind: "terminal-revert",
        },
        {
          id: "exit",
          label: "Exit",
          detail: "Flow leaves Vault.sol",
          kind: "exit",
        },
      ],
      edges: [
        { from: "entry", to: "revert", kind: "normal" },
        { from: "revert", to: "exit", label: "revert", kind: "revert" },
      ],
    });

    expect(snapshot.nodeLabels).toEqual(["Entry", "revert"]);
    expect(snapshot.edgeCount).toBe(1);
  });
});

describe("isGraphDocumentLike", () => {
  const validGraph = {
    kind: "imports",
    title: "Imports",
    focusNodeId: "main",
    nodes: [
      {
        id: "main",
        label: "Main.sol",
        detail: "Source file",
        kind: "file",
      },
      {
        id: "dep",
        label: "Dep.sol",
        detail: "Source file",
        kind: "file",
      },
    ],
    edges: [{ from: "main", to: "dep", label: "imports", kind: "imports" }],
  };

  it("accepts a complete graph with declared optional fields", () => {
    expect(isGraphDocumentLike(validGraph)).toBe(true);
  });

  it("rejects malformed optional labels and arbitrary edge kinds", () => {
    expect(
      isGraphDocumentLike({
        ...validGraph,
        edges: [{ from: "main", to: "dep", label: { unsafe: true } }],
      })
    ).toBe(false);
    expect(
      isGraphDocumentLike({
        ...validGraph,
        edges: [{ from: "main", to: "dep", kind: 'normal" onclick="x' }],
      })
    ).toBe(false);
  });

  it("rejects unknown node kinds, duplicate nodes, and dangling edges", () => {
    expect(
      isGraphDocumentLike({
        ...validGraph,
        nodes: [{ ...validGraph.nodes[0], kind: "unknown" }],
        edges: [],
      })
    ).toBe(false);
    expect(
      isGraphDocumentLike({
        ...validGraph,
        nodes: [validGraph.nodes[0], validGraph.nodes[0]],
        edges: [],
      })
    ).toBe(false);
    expect(
      isGraphDocumentLike({
        ...validGraph,
        edges: [{ from: "main", to: "missing" }],
      })
    ).toBe(false);
  });
});
