import { describe, expect, it } from "vitest";
import { TestLspClient } from "./client";

describe("TestLspClient framing", () => {
  it("preserves UTF-8 when a multibyte code point is split across chunks", async () => {
    const client = new TestLspClient();
    const message = {
      jsonrpc: "2.0",
      method: "test/unicode",
      params: { label: "déployer 🚀" },
    };
    const body = Buffer.from(JSON.stringify(message), "utf8");
    const frame = Buffer.concat([
      Buffer.from(`Content-Length: ${body.length}\r\n\r\n`, "ascii"),
      body,
    ]);
    const rocket = Buffer.from("🚀", "utf8");
    const rocketStart = frame.indexOf(rocket);
    expect(rocketStart).toBeGreaterThan(0);
    const splitAt = rocketStart + 1;
    const received = client.waitForNotification("test/unicode");

    client.feedServerBytesForTests(frame.subarray(0, splitAt));
    client.feedServerBytesForTests(frame.subarray(splitAt));

    await expect(received).resolves.toEqual({ label: "déployer 🚀" });
  });

  it("does not report quiescence until a matching notification was observed", async () => {
    const client = new TestLspClient();
    let resolved = false;
    const settled = client
      .waitForNotificationQuiescence("test/settle", undefined, 10, 1000)
      .then(() => {
        resolved = true;
      });

    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(resolved).toBe(false);

    const body = Buffer.from(
      JSON.stringify({ jsonrpc: "2.0", method: "test/settle", params: {} }),
      "utf8"
    );
    client.feedServerBytesForTests(
      Buffer.concat([
        Buffer.from(`Content-Length: ${body.length}\r\n\r\n`, "ascii"),
        body,
      ])
    );
    await settled;
    expect(resolved).toBe(true);
  });
});
