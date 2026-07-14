export type SecurityAnalysisRerunResult =
  | { status: "complete" }
  | { status: "unconfirmed"; message: string }
  | { status: "failed"; message: string };

export const SECURITY_ANALYSIS_UNAVAILABLE_MESSAGE =
  "Security analysis is unavailable because the solgrid language server is not running. Check solgrid.path and reload VS Code.";

export async function requestSecurityAnalysisRerun(
  executeCommand: () => Promise<void>,
  sendCompatibilityRefresh: () => Promise<void>
): Promise<SecurityAnalysisRerunResult> {
  try {
    await executeCommand();
    return { status: "complete" };
  } catch (commandError) {
    try {
      await sendCompatibilityRefresh();
      return {
        status: "unconfirmed",
        message: `Security analysis could not confirm completion because the server rejected the rerun command (${errorDetail(commandError)}). A compatibility refresh was sent; results may still update.`,
      };
    } catch (fallbackError) {
      return {
        status: "failed",
        message: `Security analysis failed: ${errorDetail(commandError)}. The compatibility refresh also failed: ${errorDetail(fallbackError)}.`,
      };
    }
  }
}

function errorDetail(error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message.trim();
  }
  const detail = String(error).trim();
  return detail || "unknown error";
}
