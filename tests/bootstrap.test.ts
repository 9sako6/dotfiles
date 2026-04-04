import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import { describe, expect, test } from "bun:test";
import { spawn } from "node:child_process";
import path from "node:path";
import { withTempDir } from "./test-helpers";

function runCommand(
  command: string,
  args: string[],
  env: NodeJS.ProcessEnv = process.env,
  options: { cwd?: string } = {},
) {
  return new Promise<{ code: number | null; stderr: string; stdout: string }>((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += String(chunk);
    });
    child.stderr.on("data", (chunk) => {
      stderr += String(chunk);
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      resolve({ code, stderr, stdout });
    });
  });
}

describe("bootstrap behavior", () => {
  test("install.sh trusts the repo, runs setup, then refreshes global tools", async () => {
    await withTempDir("install-script", async (tempDir) => {
      const homeDir = path.join(tempDir, "home");
      const repoDir = path.join(tempDir, "dotfiles");
      const commandLogPath = path.join(tempDir, "mise-commands.log");
      const misePath = path.join(homeDir, ".local", "bin", "mise");

      await mkdir(path.join(repoDir, ".git"), { recursive: true });
      await mkdir(path.dirname(misePath), { recursive: true });
      await writeFile(
        misePath,
        `#!/bin/sh
printf '%s\n' "$*" >> "${commandLogPath}"
`,
      );
      await chmod(misePath, 0o755);

      const result = await runCommand("sh", ["install.sh"], {
        ...process.env,
        DOTFILES_DIR: repoDir,
        HOME: homeDir,
      }, { cwd: process.cwd() });

      expect(result.code).toBe(0);
      expect(result.stderr).toBe("");
      expect(await readFile(commandLogPath, "utf8")).toBe("trust\ninstall\nrun setup\ninstall\n");
    });
  });
});
