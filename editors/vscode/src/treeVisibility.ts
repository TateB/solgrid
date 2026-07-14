export interface InMemoryTreeRefreshTarget {
  refreshTreeData(): void;
}

export function refreshTreeWhenVisible(
  visible: boolean,
  target: InMemoryTreeRefreshTarget
): void {
  if (visible) {
    target.refreshTreeData();
  }
}
