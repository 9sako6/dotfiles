import { describe, expect, test } from "bun:test";
import { chmod, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");
const installSystemLibrary = path.join(repoRoot, "lib", "install-system.sh");

async function makeExecutable(filePath: string, content: string) {
  await writeFile(filePath, content);
  await chmod(filePath, 0o755);
}

async function runHomebrewMissingPlan(
  brewBin: string,
  exitStatus: number,
  output: string,
) {
  const process = Bun.spawn([
    "/bin/sh",
    "-c",
    '. "$1"; install_system_show_homebrew_missing "$2" /Brewfile',
    "homebrew-plan-test",
    installSystemLibrary,
    brewBin,
  ], {
    env: {
      ...Bun.env,
      BREW_EXIT_STATUS: String(exitStatus),
      BREW_OUTPUT: output,
    },
    stderr: "pipe",
    stdout: "pipe",
  });
  const [exitCode, stderr, stdout] = await Promise.all([
    process.exited,
    new Response(process.stderr).text(),
    new Response(process.stdout).text(),
  ]);
  return { exitCode, stderr, stdout };
}

describe("Homebrew dependency plan", () => {
  test("未導入dependencyを表示し、候補ありのstatusを成功として扱う", async () => {
    await withTempDir("homebrew-missing-plan", async (tempDir) => {
      const brewBin = path.join(tempDir, "brew");
      await makeExecutable(brewBin, `#!/bin/sh
printf '%s\n' "$BREW_OUTPUT"
exit "$BREW_EXIT_STATUS"
`);

      const result = await runHomebrewMissingPlan(
        brewBin,
        1,
        "Missing dependencies:\nghostty",
      );

      expect(result).toEqual({
        exitCode: 0,
        stderr: "",
        stdout: "Missing dependencies:\nghostty\n",
      });
    });
  });

  test("不足がなく出力もなければnoneを表示する", async () => {
    await withTempDir("homebrew-complete-plan", async (tempDir) => {
      const brewBin = path.join(tempDir, "brew");
      await makeExecutable(brewBin, `#!/bin/sh
exit "$BREW_EXIT_STATUS"
`);

      const result = await runHomebrewMissingPlan(brewBin, 0, "");

      expect(result).toEqual({ exitCode: 0, stderr: "", stdout: "none\n" });
    });
  });

  test("bundle check自体の失敗はplan失敗として扱う", async () => {
    await withTempDir("homebrew-plan-failure", async (tempDir) => {
      const brewBin = path.join(tempDir, "brew");
      await makeExecutable(brewBin, `#!/bin/sh
printf '%s\n' "$BREW_OUTPUT" >&2
exit "$BREW_EXIT_STATUS"
`);

      const result = await runHomebrewMissingPlan(brewBin, 2, "unexpected failure");

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("Homebrew dependency plan failed");
    });
  });
});
