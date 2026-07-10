import { describe, expect, it, vi } from "vitest";
import { refreshTreeWhenVisible } from "./treeVisibility";

describe("refreshTreeWhenVisible", () => {
  it("re-emits in-memory tree data when a hidden view becomes visible", () => {
    const refreshTreeData = vi.fn();

    refreshTreeWhenVisible(true, { refreshTreeData });

    expect(refreshTreeData).toHaveBeenCalledOnce();
  });

  it("does not refresh while the view remains hidden", () => {
    const refreshTreeData = vi.fn();

    refreshTreeWhenVisible(false, { refreshTreeData });

    expect(refreshTreeData).not.toHaveBeenCalled();
  });
});
