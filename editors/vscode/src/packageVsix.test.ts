import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, expect, it } from "vitest";

const { stageBundledBinary } = require("../scripts/package-vsix.cjs") as {
  stageBundledBinary: (
    options: { target?: string },
    context: {
      extensionRoot: string;
      repoRoot: string;
      hostTarget: string;
      solgridBin?: string;
    }
  ) => Promise<() => Promise<void>>;
};

function elfBinary(machine: number): Buffer {
  const binary = Buffer.alloc(64);
  binary.set([0x7f, 0x45, 0x4c, 0x46]);
  binary[4] = 2;
  binary[5] = 1;
  binary.writeUInt16LE(machine, 18);
  return binary;
}

function machOBinary(cpuType: number): Buffer {
  const binary = Buffer.alloc(64);
  binary.writeUInt32LE(0xfeedfacf, 0);
  binary.writeUInt32LE(cpuType, 4);
  return binary;
}

function writeExecutable(filePath: string, content: Buffer | string): void {
  fs.writeFileSync(
    filePath,
    typeof content === "string" ? content : Uint8Array.from(content),
    { mode: 0o755 }
  );
}

describe("stageBundledBinary", () => {
  it("selects target-addressed Unix input instead of another platform", async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "solgrid-vsix-"));
    const extensionRoot = path.join(root, "extension");
    const binRoot = path.join(extensionRoot, "bin");
    fs.mkdirSync(path.join(binRoot, "darwin-arm64"), { recursive: true });
    fs.mkdirSync(path.join(binRoot, "linux-x64"), { recursive: true });
    const darwinBinary = machOBinary(0x0100000c);
    const linuxBinary = elfBinary(62);
    writeExecutable(
      path.join(binRoot, "darwin-arm64", "solgrid"),
      darwinBinary
    );
    writeExecutable(path.join(binRoot, "linux-x64", "solgrid"), linuxBinary);

    try {
      const cleanup = await stageBundledBinary(
        { target: "linux-x64" },
        {
          extensionRoot,
          repoRoot: root,
          hostTarget: "darwin-arm64",
        }
      );
      expect(fs.readFileSync(path.join(binRoot, "solgrid"))).toEqual(
        linuxBinary
      );
      await cleanup();
      expect(fs.existsSync(path.join(binRoot, "solgrid"))).toBe(false);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it("refuses an unverified flat Unix binary for a different target", async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "solgrid-vsix-"));
    const extensionRoot = path.join(root, "extension");
    const binRoot = path.join(extensionRoot, "bin");
    fs.mkdirSync(binRoot, { recursive: true });
    fs.writeFileSync(path.join(binRoot, "solgrid"), "darwin");

    try {
      await expect(
        stageBundledBinary(
          { target: "linux-x64" },
          {
            extensionRoot,
            repoRoot: root,
            hostTarget: "darwin-arm64",
          }
        )
      ).rejects.toThrow(/unverified flat/u);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it("rejects an explicit binary for the wrong native target", async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "solgrid-vsix-"));
    const extensionRoot = path.join(root, "extension");
    const explicitBinary = path.join(root, "solgrid");
    writeExecutable(explicitBinary, machOBinary(0x0100000c));

    try {
      await expect(
        stageBundledBinary(
          { target: "linux-x64" },
          {
            extensionRoot,
            repoRoot: root,
            hostTarget: "darwin-arm64",
            solgridBin: explicitBinary,
          }
        )
      ).rejects.toThrow(/does not match linux-x64/u);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it("does not require the packaging host to be a supported target", async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "solgrid-vsix-"));
    const extensionRoot = path.join(root, "extension");
    const explicitBinary = path.join(root, "solgrid");
    const linuxBinary = elfBinary(62);
    writeExecutable(explicitBinary, linuxBinary);

    try {
      const cleanup = await stageBundledBinary(
        { target: "linux-x64" },
        {
          extensionRoot,
          repoRoot: root,
          hostTarget: "darwin-x64",
          solgridBin: explicitBinary,
        }
      );
      expect(
        fs.readFileSync(path.join(extensionRoot, "bin", "solgrid"))
      ).toEqual(linuxBinary);
      await cleanup();
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it("rejects a non-binary target-addressed input", async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "solgrid-vsix-"));
    const extensionRoot = path.join(root, "extension");
    const binaryPath = path.join(
      extensionRoot,
      "bin",
      "linux-x64",
      "solgrid"
    );
    fs.mkdirSync(path.dirname(binaryPath), { recursive: true });
    writeExecutable(binaryPath, "not a native executable");

    try {
      await expect(
        stageBundledBinary(
          { target: "linux-x64" },
          {
            extensionRoot,
            repoRoot: root,
            hostTarget: "darwin-arm64",
          }
        )
      ).rejects.toThrow(/unrecognized native executable format/u);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
