import { describe, expect, test } from "bun:test";
import { chmod, mkdir, readFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers";

function runInstallScript(env: NodeJS.ProcessEnv) {
  return new Promise<{ code: number | null; stderr: string }>((resolve, reject) => {
    const child = spawn("/bin/sh", [path.resolve("install.sh")], {
      env,
      stdio: ["ignore", "ignore", "pipe"],
    });
    let stderr = "";
    child.stderr.on("data", (chunk) => {
      stderr += String(chunk);
    });
    child.on("error", reject);
    child.on("exit", (code) => resolve({ code, stderr }));
  });
}

describe("install.sh", () => {
  test("trusts the repo and delegates setup to mise bootstrap", async () => {
    await withTempDir("setup", async (tempDir) => {
      const homeDir = path.join(tempDir, "home");
      const repoDir = path.join(tempDir, "dotfiles");
      const miseBin = path.join(homeDir, ".local", "bin", "mise");
      const miseLog = path.join(tempDir, "mise.log");

      await mkdir(path.join(repoDir, ".git"), { recursive: true });
      await writeTree(path.dirname(miseBin), {
        mise: `#!/bin/sh
set -eu
if [ "\${1:-}" = "--version" ]; then
  printf '%s\n' '2026.7.7 macos-arm64 (test)'
  exit 0
fi
printf '%s\n' "$*" >> "$MISE_TEST_LOG"
`,
      });
      await chmod(miseBin, 0o755);

      const result = await runInstallScript({
        ...process.env,
        DOTFILES_DIR: repoDir,
        HOME: homeDir,
        MISE_TEST_LOG: miseLog,
        PATH: "/usr/bin:/bin",
      });

      expect(result.code).toBe(0);
      expect(await readFile(miseLog, "utf8")).toBe("trust\nbootstrap --yes\n");
    });
  });
});
