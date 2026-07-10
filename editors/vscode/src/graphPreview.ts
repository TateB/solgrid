import * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";
import {
  assessGraphRenderBudget,
  buildGraphPreviewSnapshot,
  GraphDocumentLike,
  GraphKind,
  GraphPreviewSnapshot,
  isGraphDocumentLike,
  renderGraphWebviewHtml,
} from "./graphPreviewRender";

export interface GraphCommandArgs {
  kind: GraphKind;
  uri: string;
  symbolName?: string;
  targetOffset?: number;
}

export interface GraphPreviewRequestSnapshot extends GraphPreviewSnapshot {
  requestId: number;
  requestKey: string;
}

export type GraphStatusKind = "loading" | "empty" | "error" | "limit";

export interface GraphStatusView {
  kind: GraphStatusKind;
  title: string;
  message: string;
}

let graphPanel: vscode.WebviewPanel | undefined;
let lastGraphPreviewSnapshot: GraphPreviewRequestSnapshot | undefined;
let graphRequestSequence = 0;

export function getGraphPreviewSnapshot():
  | GraphPreviewRequestSnapshot
  | undefined {
  return lastGraphPreviewSnapshot;
}

export async function showGraph(
  client: LanguageClient | undefined,
  args?: GraphCommandArgs
): Promise<void> {
  const request = args ?? activeImportsGraphArgs();
  if (!request) {
    return;
  }

  const requestSequence = ++graphRequestSequence;
  const requestKey = graphRequestKey(request);
  const subject = graphSubject(request);
  lastGraphPreviewSnapshot = undefined;
  showGraphStatusPanel({
    kind: "loading",
    title: `Building ${subject}`,
    message: "Waiting for the solgrid language server to build the graph.",
  });

  if (!client) {
    showGraphStatusPanel({
      kind: "error",
      title: `Could not build ${subject}`,
      message:
        "The solgrid language server is unavailable. Check the configured binary and reload VS Code.",
    });
    return;
  }

  const command = graphCommand(request.kind);
  let graph: GraphDocumentLike | null;
  try {
    graph = await client.sendRequest<GraphDocumentLike | null>(
      "workspace/executeCommand",
      {
        command,
        arguments: [
          {
            uri: request.uri,
            symbolName: request.symbolName ?? null,
            targetOffset: request.targetOffset ?? null,
          },
        ],
      }
    );
  } catch (error) {
    if (requestSequence === graphRequestSequence) {
      const detail = error instanceof Error ? error.message : String(error);
      showGraphStatusPanel({
        kind: "error",
        title: `Could not build ${subject}`,
        message: detail,
      });
      void vscode.window.showErrorMessage(
        `solgrid could not build ${subject}: ${detail}`
      );
    }
    return;
  }

  if (requestSequence !== graphRequestSequence) {
    return;
  }

  if (!graph) {
    showGraphStatusPanel({
      kind: "empty",
      title: `No graph available for ${subject}`,
      message:
        "solgrid returned no graph data for this source location. Try another file or symbol.",
    });
    void vscode.window.showWarningMessage(`solgrid could not build ${subject}.`);
    return;
  }

  if (!isGraphDocumentLike(graph)) {
    showGraphStatusPanel({
      kind: "error",
      title: `Could not build ${subject}`,
      message: "The language server returned malformed graph data.",
    });
    void vscode.window.showWarningMessage(`solgrid could not build ${subject}.`);
    return;
  }

  const renderBudget = assessGraphRenderBudget(graph);
  if (!renderBudget.canRender) {
    showGraphStatusPanel({
      kind: "limit",
      title: "Graph too large to render",
      message: [
        `This graph contains ${countLabel(renderBudget.nodeCount, "node")} and ${countLabel(renderBudget.edgeCount, "relationship")}.`,
        "The preview was not rendered or silently truncated, so it does not present an incomplete graph.",
        `The interactive view is limited to ${renderBudget.nodeLimit} nodes, ${renderBudget.edgeLimit} relationships, and ${renderBudget.textLimit.toLocaleString("en-US")} characters of graph content to keep VS Code responsive.`,
        "Narrow the request to a specific symbol or source file, or use the solgrid graph CLI to export the complete graph.",
      ].join(" "),
    });
    return;
  }

  showGraphPanel(graph, requestSequence, requestKey);
}

export function graphRequestKey(request: GraphCommandArgs): string {
  return JSON.stringify([
    request.kind,
    request.uri,
    request.symbolName ?? null,
    request.targetOffset ?? null,
  ]);
}

function graphCommand(kind: GraphKind): string {
  switch (kind) {
    case "imports":
      return "solgrid.graph.imports";
    case "inheritance":
      return "solgrid.graph.inheritance";
    case "linearized-inheritance":
      return "solgrid.graph.linearizedInheritance";
    case "control-flow":
      return "solgrid.graph.controlFlow";
  }
}

function graphSubject(request: GraphCommandArgs): string {
  switch (request.kind) {
    case "imports":
      return "an imports graph";
    case "inheritance":
      return `an inheritance graph${request.symbolName ? ` for ${request.symbolName}` : ""}`;
    case "linearized-inheritance":
      return `a linearized inheritance graph${
        request.symbolName ? ` for ${request.symbolName}` : ""
      }`;
    case "control-flow":
      return `a control-flow graph${request.symbolName ? ` for ${request.symbolName}` : ""}`;
  }
}

function ensureGraphPanel(title: string): vscode.WebviewPanel {
  if (!graphPanel) {
    graphPanel = vscode.window.createWebviewPanel(
      "solgridGraphPreview",
      title,
      vscode.ViewColumn.Beside,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
      }
    );
    graphPanel.onDidDispose(() => {
      graphPanel = undefined;
      lastGraphPreviewSnapshot = undefined;
      graphRequestSequence += 1;
    });
    graphPanel.webview.onDidReceiveMessage((message) => {
      if (
        message &&
        typeof message === "object" &&
        message.type === "openSource" &&
        typeof message.uri === "string"
      ) {
        let uri: vscode.Uri;
        try {
          uri = vscode.Uri.parse(message.uri, true);
        } catch {
          return;
        }
        if (uri.scheme !== "file") {
          void vscode.window.showWarningMessage(
            "solgrid graph source links may only open local files."
          );
          return;
        }
        void vscode.commands.executeCommand("vscode.open", uri);
      }
    });
  }

  return graphPanel;
}

function showGraphStatusPanel(status: GraphStatusView): void {
  const panel = ensureGraphPanel(status.title);
  panel.title = status.title;
  panel.webview.html = renderGraphStatusWebviewHtml(status, {
    cspSource: panel.webview.cspSource,
    nonce: createNonce(),
  });
  panel.reveal(vscode.ViewColumn.Beside, false);
}

function showGraphPanel(
  graph: GraphDocumentLike,
  requestId: number,
  requestKey: string
): void {
  const panel = ensureGraphPanel(graph.title);

  panel.title = graph.title;
  panel.webview.html = renderGraphWebviewHtml(graph, {
    cspSource: panel.webview.cspSource,
    nonce: createNonce(),
  });
  panel.reveal(vscode.ViewColumn.Beside, false);
  lastGraphPreviewSnapshot = {
    ...buildGraphPreviewSnapshot(graph),
    requestId,
    requestKey,
  };
}

export function renderGraphStatusWebviewHtml(
  status: GraphStatusView,
  options: { cspSource: string; nonce: string }
): string {
  const role = status.kind === "error" ? "alert" : "status";
  const icon =
    status.kind === "loading" ? "" : status.kind === "empty" ? "○" : "!";
  const iconMarkup = icon
    ? `<span class="status-icon" aria-hidden="true">${icon}</span>`
    : '<span class="status-spinner" aria-hidden="true"></span>';
  const policy = escapeHtml(
    `default-src 'none'; style-src ${options.cspSource} 'nonce-${options.nonce}';`
  );
  const nonce = escapeHtml(options.nonce);

  return `<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8">
    <meta
      http-equiv="Content-Security-Policy"
      content="${policy}"
    >
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>${escapeHtml(status.title)}</title>
    <style nonce="${nonce}">
      :root { color-scheme: light dark; }
      * { box-sizing: border-box; }
      body {
        min-height: 100vh;
        margin: 0;
        display: grid;
        place-items: center;
        padding: 32px;
        color: var(--vscode-foreground);
        background: var(--vscode-editor-background);
        font-family: var(--vscode-font-family);
      }
      body.vscode-light,
      body.vscode-high-contrast-light { color-scheme: light; }
      body.vscode-dark,
      body.vscode-high-contrast { color-scheme: dark; }
      main {
        width: min(520px, 100%);
        display: grid;
        justify-items: center;
        gap: 12px;
        padding: 28px;
        text-align: center;
        border: 1px solid var(--vscode-panel-border, var(--vscode-widget-border));
        border-radius: 8px;
        background: var(--vscode-sideBar-background, var(--vscode-editor-background));
      }
      body.vscode-high-contrast main,
      body.vscode-high-contrast-light main {
        border-color: var(--vscode-contrastBorder, var(--vscode-panel-border, var(--vscode-widget-border)));
      }
      h1 { margin: 0; font-size: 18px; line-height: 1.35; }
      p {
        max-width: 60ch;
        margin: 0;
        color: var(--vscode-descriptionForeground);
        line-height: 1.5;
      }
      .status-icon,
      .status-spinner {
        width: 28px;
        height: 28px;
        display: grid;
        place-items: center;
        font-size: 20px;
        font-weight: 700;
      }
      .status-icon { color: var(--vscode-descriptionForeground); }
      main[data-kind="error"] .status-icon {
        color: var(--vscode-notificationsErrorIcon-foreground, var(--vscode-errorForeground));
      }
      main[data-kind="limit"] .status-icon {
        color: var(--vscode-notificationsWarningIcon-foreground, var(--vscode-editorWarning-foreground));
      }
      .status-spinner {
        border: 2px solid var(--vscode-progressBar-background);
        border-right-color: transparent;
        border-radius: 50%;
        animation: spin 0.9s linear infinite;
      }
      @keyframes spin { to { transform: rotate(360deg); } }
      @media (prefers-reduced-motion: reduce) {
        .status-spinner { animation: none; border-right-color: var(--vscode-progressBar-background); }
      }
    </style>
  </head>
  <body>
    <main data-kind="${status.kind}" role="${role}" aria-live="${status.kind === "error" ? "assertive" : "polite"}">
      ${iconMarkup}
      <h1>${escapeHtml(status.title)}</h1>
      <p>${escapeHtml(status.message)}</p>
    </main>
  </body>
</html>`;
}

export function activeImportsGraphArgs(): GraphCommandArgs | undefined {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== "solidity") {
    void vscode.window.showWarningMessage(
      "Open a Solidity file before requesting a solgrid graph."
    );
    return undefined;
  }

  return {
    kind: "imports",
    uri: editor.document.uri.toString(),
  };
}

function createNonce(): string {
  return `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`;
}

function countLabel(count: number, singular: string): string {
  return `${count} ${count === 1 ? singular : `${singular}s`}`;
}

function escapeHtml(value: string): string {
  return value.replace(
    /[&<>"']/g,
    (character) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&#39;",
      })[character] ?? character
  );
}
