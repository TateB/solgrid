import { describe, expect, it } from "vitest";
import {
  buildGraphPreviewSnapshot,
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
    expect(html).toContain("Vault");
    expect(html).toContain("Ownable");
    expect(html).toContain("marker-end=\"url(#arrow)\"");
    expect(html).not.toContain("```mermaid");
  });

  it("renders explicit linearization metadata in the sidebar", () => {
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

    expect(html).toContain("Linearization");
    expect(html).toContain("Order: Vault -&gt; AccessControl -&gt; Context");
    expect(html).toContain("lineage-index");
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
    expect(snapshot.summary).toContain("Function-level CFG");
  });
});
