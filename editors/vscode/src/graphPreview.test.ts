import { describe, expect, it } from "vitest";
import { renderGraphMarkdown } from "./graphPreviewRender";

describe("renderGraphMarkdown", () => {
  it("renders a mermaid graph and node summary", () => {
    const markdown = renderGraphMarkdown({
      kind: "inheritance",
      title: "Inheritance graph for Vault",
      focusNodeId: "Vault",
      nodes: [
        {
          id: "Vault",
          label: "Vault",
          detail: "Contract in src/Vault.sol",
          kind: "contract",
          uri: "file:///workspace/src/Vault.sol",
        },
        {
          id: "Ownable",
          label: "Ownable",
          detail: "Contract in lib/Ownable.sol",
          kind: "contract",
          uri: "file:///workspace/lib/Ownable.sol",
        },
      ],
      edges: [
        {
          from: "Vault",
          to: "Ownable",
          label: "inherits",
          kind: "inherits",
        },
      ],
    });

    expect(markdown).toContain("# Inheritance graph for Vault");
    expect(markdown).toContain("```mermaid");
    expect(markdown).toContain('n0["Vault"]');
    expect(markdown).toContain("n0 -->|inherits| n1");
    expect(markdown).toContain("style n0 fill:#d9f2e6");
    expect(markdown).toContain("class n0,n1 contract");
    expect(markdown).toContain("## Nodes");
    expect(markdown).toContain("**Vault** `contract`: Contract in src/Vault.sol");
  });

  it("renders linearized inheritance order explicitly", () => {
    const markdown = renderGraphMarkdown({
      kind: "linearized-inheritance",
      title: "Linearized inheritance for Vault",
      focusNodeId: "Vault",
      nodes: [
        {
          id: "Vault",
          label: "Vault",
          detail: "#1 Contract in src/Vault.sol",
        },
        {
          id: "AccessControl",
          label: "AccessControl",
          detail: "#2 Contract in src/AccessControl.sol",
        },
        {
          id: "Context",
          label: "Context",
          detail: "#3 Contract in src/Context.sol",
        },
      ],
      edges: [
        {
          from: "Vault",
          to: "AccessControl",
          label: "precedes",
        },
        {
          from: "AccessControl",
          to: "Context",
          label: "precedes",
        },
      ],
    });

    expect(markdown).toContain("Order: Vault -> AccessControl -> Context");
    expect(markdown).toContain("## Linearization");
    expect(markdown).toContain("1. `Vault`");
    expect(markdown).toContain("2. `AccessControl`");
    expect(markdown).toContain("3. `Context`");
  });

  it("renders control-flow graphs top-down", () => {
    const markdown = renderGraphMarkdown({
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
          id: "call",
          label: "call require",
          detail: "require(amount > 0);",
          kind: "call",
        },
        {
          id: "exit",
          label: "Exit",
          detail: "Flow leaves Vault.sol",
          kind: "exit",
        },
      ],
      edges: [
        {
          from: "entry",
          to: "if",
          kind: "normal",
        },
        {
          from: "if",
          to: "call",
          label: "true",
          kind: "branch-true",
        },
        {
          from: "call",
          to: "exit",
          label: "return",
          kind: "return",
        },
      ],
    });

    expect(markdown).toContain("# Control-flow graph for Vault.run");
    expect(markdown).toContain("flowchart TD");
    expect(markdown).toContain("including 1 branch node, 1 call/emission node");
    expect(markdown).toContain("class n0 entry");
    expect(markdown).toContain("class n1 branch");
    expect(markdown).toContain("class n2 call");
    expect(markdown).toContain("class n3 exit");
    expect(markdown).toContain("n1 -->|true| n2");
    expect(markdown).toContain("n2 ==>|return| n3");
  });
});
