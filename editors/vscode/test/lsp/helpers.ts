/**
 * LSP test helpers — convenience functions for common LSP operations.
 *
 * These helpers wrap the TestLspClient for the most common test patterns:
 * initialize, open/change/close documents, and wait for diagnostics.
 */

import * as fs from "fs";
import * as path from "path";
import { TestLspClient } from "./client";

// ---------------------------------------------------------------------------
// LSP Type Aliases (minimal, to avoid depending on vscode-languageserver-protocol)
// ---------------------------------------------------------------------------

export interface Position {
  line: number;
  character: number;
}

export interface Range {
  start: Position;
  end: Position;
}

export interface TextEdit {
  range: Range;
  newText: string;
}

export interface Diagnostic {
  range: Range;
  severity?: number;
  code?: string | number;
  source?: string;
  message: string;
}

export interface PublishDiagnosticsParams {
  uri: string;
  version?: number;
  diagnostics: Diagnostic[];
}

export interface CodeAction {
  title: string;
  kind?: string;
  isPreferred?: boolean;
  diagnostics?: Diagnostic[];
  edit?: {
    changes?: Record<string, TextEdit[]>;
  };
}

export interface Hover {
  contents:
    | string
    | { kind: string; value: string }
    | Array<string | { language: string; value: string }>;
  range?: Range;
}

export interface CompletionItem {
  label: string;
  kind?: number;
  detail?: string;
  insertText?: string;
  sortText?: string;
  additionalTextEdits?: TextEdit[];
}

export interface ParameterInformation {
  label: string | [number, number];
  documentation?: string | { kind: string; value: string };
}

export interface SignatureInformation {
  label: string;
  documentation?: string | { kind: string; value: string };
  parameters?: ParameterInformation[];
  activeParameter?: number;
}

export interface SignatureHelp {
  signatures: SignatureInformation[];
  activeSignature?: number;
  activeParameter?: number;
}

export interface InitializeResult {
  capabilities: {
    textDocumentSync?: {
      openClose?: boolean;
      change?: number;
      save?: unknown;
      willSaveWaitUntil?: boolean;
    };
    codeActionProvider?: unknown;
    documentFormattingProvider?: boolean | unknown;
    documentRangeFormattingProvider?: boolean | unknown;
    hoverProvider?: boolean | unknown;
    completionProvider?: { triggerCharacters?: string[] };
    signatureHelpProvider?: {
      triggerCharacters?: string[];
      retriggerCharacters?: string[];
    };
  };
  serverInfo?: {
    name: string;
    version?: string;
  };
}

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

const FIXTURES_DIR = path.resolve(__dirname, "../fixtures");

export function fixturePath(name: string): string {
  return path.resolve(FIXTURES_DIR, name);
}

export function fixtureUri(name: string): string {
  const absPath = fixturePath(name);
  return `file://${absPath}`;
}

export function readFixture(name: string): string {
  return fs.readFileSync(fixturePath(name), "utf-8");
}

// ---------------------------------------------------------------------------
// Client lifecycle helpers
// ---------------------------------------------------------------------------

/**
 * Initialize the LSP server with standard client capabilities.
 */
export async function initializeServer(
  client: TestLspClient,
  rootUri?: string,
  initializationOptions?: Record<string, unknown>
): Promise<InitializeResult> {
  const result = await client.request<InitializeResult>("initialize", {
    processId: process.pid,
    rootUri: rootUri ?? `file://${FIXTURES_DIR}`,
    initializationOptions,
    capabilities: {
      textDocument: {
        synchronization: {
          willSave: true,
          willSaveWaitUntil: true,
          didSave: true,
        },
        codeAction: {
          codeActionLiteralSupport: {
            codeActionKind: {
              valueSet: [
                "quickfix",
                "refactor",
                "refactor.rewrite",
                "source.fixAll",
              ],
            },
          },
        },
        formatting: {},
        rangeFormatting: {},
        hover: {
          contentFormat: ["markdown", "plaintext"],
        },
        completion: {
          completionItem: {
            snippetSupport: false,
          },
        },
        signatureHelp: {
          signatureInformation: {
            documentationFormat: ["markdown", "plaintext"],
            parameterInformation: {
              labelOffsetSupport: true,
            },
          },
        },
        publishDiagnostics: {},
      },
      workspace: {
        configuration: true,
      },
    },
  });

  // Send initialized notification
  client.notify("initialized", {});

  return result;
}

// ---------------------------------------------------------------------------
// Document helpers
// ---------------------------------------------------------------------------

let documentVersions = new Map<string, number>();

export function resetDocumentVersions(): void {
  documentVersions = new Map();
}

export function openDocument(
  client: TestLspClient,
  uri: string,
  content: string,
  languageId = "solidity"
): void {
  documentVersions.set(uri, 1);
  client.notify("textDocument/didOpen", {
    textDocument: {
      uri,
      languageId,
      version: 1,
      text: content,
    },
  });
}

export function changeDocument(
  client: TestLspClient,
  uri: string,
  content: string
): void {
  const version = (documentVersions.get(uri) ?? 0) + 1;
  documentVersions.set(uri, version);
  client.notify("textDocument/didChange", {
    textDocument: { uri, version },
    contentChanges: [{ text: content }],
  });
}

export function closeDocument(client: TestLspClient, uri: string): void {
  documentVersions.delete(uri);
  client.notify("textDocument/didClose", {
    textDocument: { uri },
  });
}

export function saveDocument(
  client: TestLspClient,
  uri: string,
  content: string
): void {
  client.notify("textDocument/didSave", {
    textDocument: { uri },
    text: content,
  });
}

// ---------------------------------------------------------------------------
// Diagnostic helpers
// ---------------------------------------------------------------------------

/**
 * Wait for a publishDiagnostics notification for a specific URI.
 */
export function waitForDiagnostics(
  client: TestLspClient,
  uri: string,
  timeoutMs = 15000
): Promise<PublishDiagnosticsParams> {
  return client.waitForNotification(
    "textDocument/publishDiagnostics",
    (params) => (params as PublishDiagnosticsParams).uri === uri,
    timeoutMs
  ) as Promise<PublishDiagnosticsParams>;
}

// ---------------------------------------------------------------------------
// Request helpers
// ---------------------------------------------------------------------------

export async function requestCodeActions(
  client: TestLspClient,
  uri: string,
  range: Range,
  diagnostics?: Diagnostic[]
): Promise<(CodeAction | { command: unknown })[]> {
  return (
    (await client.request("textDocument/codeAction", {
      textDocument: { uri },
      range,
      context: {
        diagnostics: diagnostics ?? [],
      },
    })) ?? []
  );
}

export async function requestFormatting(
  client: TestLspClient,
  uri: string
): Promise<TextEdit[] | null> {
  return client.request("textDocument/formatting", {
    textDocument: { uri },
    options: { tabSize: 4, insertSpaces: true },
  });
}

export async function requestRangeFormatting(
  client: TestLspClient,
  uri: string,
  range: Range
): Promise<TextEdit[] | null> {
  return client.request("textDocument/rangeFormatting", {
    textDocument: { uri },
    range,
    options: { tabSize: 4, insertSpaces: true },
  });
}

export async function requestHover(
  client: TestLspClient,
  uri: string,
  position: Position
): Promise<Hover | null> {
  return client.request("textDocument/hover", {
    textDocument: { uri },
    position,
  });
}

export async function requestCompletion(
  client: TestLspClient,
  uri: string,
  position: Position
): Promise<CompletionItem[] | { items: CompletionItem[] } | null> {
  return client.request("textDocument/completion", {
    textDocument: { uri },
    position,
  });
}

export async function requestSignatureHelp(
  client: TestLspClient,
  uri: string,
  position: Position
): Promise<SignatureHelp | null> {
  return client.request("textDocument/signatureHelp", {
    textDocument: { uri },
    position,
  });
}

export async function requestWillSaveWaitUntil(
  client: TestLspClient,
  uri: string
): Promise<TextEdit[] | null> {
  return client.request("textDocument/willSaveWaitUntil", {
    textDocument: { uri },
    reason: 1, // Manual save
  });
}

// ---------------------------------------------------------------------------
// Assertion helpers
// ---------------------------------------------------------------------------

export function fullFileRange(content: string): Range {
  const lines = content.split("\n");
  return {
    start: { line: 0, character: 0 },
    end: {
      line: Math.max(0, lines.length - 1),
      character: lines[lines.length - 1]?.length ?? 0,
    },
  };
}

/**
 * Apply an array of TextEdits to source content.
 * Assumes edits don't overlap and applies them in reverse order.
 */
export function applyEdits(source: string, edits: TextEdit[]): string {
  if (!edits || edits.length === 0) return source;

  // For a single full-document replacement, just return the new text
  if (edits.length === 1) {
    const edit = edits[0];
    const lines = source.split("\n");
    const startOffset = positionToOffset(lines, edit.range.start);
    const endOffset = positionToOffset(lines, edit.range.end);
    return source.substring(0, startOffset) + edit.newText + source.substring(endOffset);
  }

  // Sort edits in reverse order (bottom to top) to preserve offsets
  const sorted = [...edits].sort((a, b) => {
    if (a.range.start.line !== b.range.start.line) {
      return b.range.start.line - a.range.start.line;
    }
    return b.range.start.character - a.range.start.character;
  });

  let result = source;
  for (const edit of sorted) {
    const lines = result.split("\n");
    const startOffset = positionToOffset(lines, edit.range.start);
    const endOffset = positionToOffset(lines, edit.range.end);
    result =
      result.substring(0, startOffset) +
      edit.newText +
      result.substring(endOffset);
  }
  return result;
}

function positionToOffset(lines: string[], pos: Position): number {
  let offset = 0;
  for (let i = 0; i < pos.line && i < lines.length; i++) {
    offset += lines[i].length + 1; // +1 for \n
  }
  offset += Math.min(pos.character, lines[pos.line]?.length ?? 0);
  return offset;
}
