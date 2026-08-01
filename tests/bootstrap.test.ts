import { describe, expect, test } from "bun:test";
import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");
const installScript = path.join(repoRoot, "install.sh");

async function makeExecutable(filePath: string, content: string) {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
  await chmod(filePath, 0o755);
}

async function prepareBootstrapEnvironment(tempDir: string) {
  const dotfilesDir = path.join(tempDir, "dotfiles");
  const homeDir = path.join(tempDir, "home");
  const logPath = path.join(tempDir, "bootstrap.log");

  await mkdir(path.join(dotfilesDir, ".git"), { recursive: true });
  await makeExecutable(
    path.join(dotfilesDir, "scripts/install-mise.sh"),
    `#!/bin/sh
printf 'install-mise\\n' >> "$BOOTSTRAP_LOG"
`,
  );
  await makeExecutable(
    path.join(homeDir, ".local/bin/mise"),
    `#!/bin/sh
printf 'mise' >> "$BOOTSTRAP_LOG"
printf ' <%s>' "$@" >> "$BOOTSTRAP_LOG"
printf '\\n' >> "$BOOTSTRAP_LOG"
`,
  );

  return {
    env: {
      ...process.env,
      BOOTSTRAP_LOG: logPath,
      DOTFILES_DIR: dotfilesDir,
      HOME: homeDir,
    },
    logPath,
  };
}

async function runScript(script: string, env: NodeJS.ProcessEnv) {
  const proc = Bun.spawn(["/bin/sh", script], {
    cwd: repoRoot,
    env,
    stderr: "pipe",
    stdout: "pipe",
  });
  const [exitCode, stderr, stdout] = await Promise.all([
    proc.exited,
    new Response(proc.stderr).text(),
    new Response(proc.stdout).text(),
  ]);
  return { exitCode, stderr, stdout };
}

describe("公開bootstrap", () => {
  test("取得が完了したinstall.shだけがbootstrapを開始する", async () => {
    await withTempDir("bootstrap-complete", async (tempDir) => {
      const { env, logPath } = await prepareBootstrapEnvironment(tempDir);

      const result = await runScript(installScript, env);

      expect(result).toEqual({ exitCode: 0, stderr: "", stdout: "" });
      expect(await readFile(logPath, "utf8")).toBe(
        "install-mise\nmise <trust>\nmise <bootstrap> <--yes>\n",
      );
    });
  });

  test("install.shが最終行より前で途切れたら副作用を起こさない", async () => {
    await withTempDir("bootstrap-truncated", async (tempDir) => {
      const { env, logPath } = await prepareBootstrapEnvironment(tempDir);
      const script = await readFile(installScript, "utf8");
      const previousLineEnd = script.lastIndexOf("\n", script.length - 2);
      const truncatedScript = path.join(tempDir, "install-truncated.sh");
      await writeTree(tempDir, {
        "install-truncated.sh": script.slice(0, previousLineEnd + 1),
      });

      const result = await runScript(truncatedScript, env);

      expect(result).toEqual({ exitCode: 0, stderr: "", stdout: "" });
      expect(await Bun.file(logPath).exists()).toBe(false);
    });
  });
});
