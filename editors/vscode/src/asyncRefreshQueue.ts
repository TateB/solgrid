/** Coalesces overlapping refresh requests while making every caller await the drain. */
export class AsyncRefreshQueue {
  private running: Promise<void> | undefined;
  private queued = false;

  constructor(private readonly refresh: () => Promise<void>) {}

  run(): Promise<void> {
    this.queued = true;
    if (!this.running) {
      this.running = this.drain();
    }
    return this.running;
  }

  private async drain(): Promise<void> {
    try {
      while (this.queued) {
        this.queued = false;
        await this.refresh();
      }
    } finally {
      this.running = undefined;
    }
  }
}
