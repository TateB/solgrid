interface RefreshWaiter {
  resolve: () => void;
  reject: (error: unknown) => void;
}

/** Coalesces overlapping requests while each caller awaits its covering iteration. */
export class AsyncRefreshQueue {
  private draining = false;
  private pending: RefreshWaiter[] = [];

  constructor(private readonly refresh: () => Promise<void>) {}

  run(): Promise<void> {
    const completion = new Promise<void>((resolve, reject) => {
      this.pending.push({ resolve, reject });
    });
    if (!this.draining) {
      this.draining = true;
      void this.drain();
    }
    return completion;
  }

  private async drain(): Promise<void> {
    try {
      while (this.pending.length > 0) {
        const waiters = this.pending.splice(0);
        try {
          await this.refresh();
          for (const waiter of waiters) {
            waiter.resolve();
          }
        } catch (error) {
          for (const waiter of waiters) {
            waiter.reject(error);
          }
        }
      }
    } finally {
      this.draining = false;
    }
  }
}
