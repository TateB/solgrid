import { describe, expect, it } from "vitest";
import {
  buildGraphPreviewSnapshot,
  isGraphDocumentLike,
  renderGraphWebviewHtml,
} from "./graphPreviewRender";

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
    expect(html).toContain("data-source-uri=\"file:///workspace/src/Vault.sol\"");
    expect(html).toContain("Vault");
    expect(html).toContain("Ownable");
    expect(html).toContain("marker-end=\"url(#arrow)\"");
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
    expect(html).toContain("data-graph-width=\"900\"");
    expect(html).not.toContain("node-list");
    expect(html).not.toContain("lineage-index");
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
    expect(html).toContain("data-zoom-action=\"fit\"");
    expect(html).toContain("canvasWrap.addEventListener(\"pointerdown\", beginPan);");
    expect(html).toContain("canvasWrap.addEventListener(\"dblclick\"");
    expect(html).toContain("\"wheel\"");
    expect(html).toContain("const WHEEL_ZOOM_SENSITIVITY = 0.002;");
    expect(html).toContain("const TRACKPAD_ZOOM_SENSITIVITY = 0.004;");
    expect(html).toContain("const zoomFactor = wheelZoomFactor(event);");
    expect(html).toContain("setGraphScale(fitScale(), { preserveCenter: false });");
    expect(html).toContain("typeof acquireVsCodeApi === \"function\"");
    expect(html).not.toContain("<aside");
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
    expect(html).toContain("class=\"edge-label-pill branch-true\"");
    expect(html).toContain("class=\"edge branch-false\"");
    expect(html).toContain("class=\"edge-label-pill branch-false\"");
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
        edges: [{ from: "main", to: "dep", kind: 'normal\" onclick=\"x' }],
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
