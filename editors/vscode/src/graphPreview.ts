import * as vscode from "vscode";
import { LanguageClient } from "vscode-languageclient/node";
import {
  buildGraphPreviewSnapshot,
  GraphDocumentLike,
  GraphKind,
  GraphPreviewSnapshot,
  renderGraphWebviewHtml,
} from "./graphPreviewRender";

export interface GraphCommandArgs {
  kind: GraphKind;
  uri: string;
  symbolName?: string;
  targetOffset?: number;
}

let graphPanel: vscode.WebviewPanel | undefined;
let lastGraphPreviewSnapshot: GraphPreviewSnapshot | undefined;

export function getGraphPreviewSnapshot():
  | GraphPreviewSnapshot
  | undefined {
  return lastGraphPreviewSnapshot;
}

export async function showGraph(
  client: LanguageClient | undefined,
  args?: GraphCommandArgs
): Promise<void> {
  if (!client) {
    return;
  }

  const request = args ?? activeImportsGraphArgs();
  if (!request) {
    return;
  }

  const command = graphCommand(request.kind);
  const graph = await client.sendRequest<GraphDocumentLike | null>(
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

  if (!graph) {
    const subject = graphSubject(request);
    void vscode.window.showWarningMessage(`solgrid could not build ${subject}.`);
    return;
  }

  showGraphPanel(graph);
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

function showGraphPanel(graph: GraphDocumentLike): void {
  if (!graphPanel) {
    graphPanel = vscode.window.createWebviewPanel(
      "solgridGraphPreview",
      graph.title,
      vscode.ViewColumn.Beside,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
      }
    );
    graphPanel.onDidDispose(() => {
      graphPanel = undefined;
    });
    graphPanel.webview.onDidReceiveMessage((message) => {
      if (
        message &&
        typeof message === "object" &&
        message.type === "openSource" &&
        typeof message.uri === "string"
      ) {
        void vscode.commands.executeCommand(
          "vscode.open",
          vscode.Uri.parse(message.uri)
        );
      }
    });
  }

  graphPanel.title = graph.title;
  graphPanel.webview.html = renderGraphWebviewHtml(graph, {
    cspSource: graphPanel.webview.cspSource,
    nonce: createNonce(),
  });
  graphPanel.reveal(vscode.ViewColumn.Beside, false);
  lastGraphPreviewSnapshot = buildGraphPreviewSnapshot(graph);
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
