import { describe, expect, test } from "bun:test";
import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");
const installScript = path.join(repoRoot, "install.sh");
const trustedRevision = "f3ca1669a49014bff282a3868c290cda91005b8a";

async function makeExecutable(filePath: string, content: string) {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
  await chmod(filePath, 0o755);
}

async function prepareBootstrapEnvironment(
  tempDir: string,
  options: { checkoutExists?: boolean; dirty?: boolean; revision?: string } = {},
) {
  const dotfilesDir = path.join(tempDir, "dotfiles");
  const fakeBin = path.join(tempDir, "bin");
  const gitLogPath = path.join(tempDir, "git.log");
  const homeDir = path.join(tempDir, "home");
  const logPath = path.join(tempDir, "bootstrap.log");

  if (options.checkoutExists !== false) {
    await mkdir(path.join(dotfilesDir, ".git"), { recursive: true });
  }
  await makeExecutable(
    path.join(dotfilesDir, "bin/install-mise.sh"),
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
  await makeExecutable(
    path.join(fakeBin, "git"),
    `#!/bin/sh
set -eu
printf '<%s>' "$@" >> "$BOOTSTRAP_GIT_LOG"
printf '\\n' >> "$BOOTSTRAP_GIT_LOG"

if [ "$1" = "clone" ]; then
  mkdir -p "$4/.git"
  exit 0
fi

shift 2
case "$1" in
  checkout) exit 0 ;;
  rev-parse) printf '%s\\n' "$BOOTSTRAP_REVISION" ;;
  status) [ "${options.dirty ? "1" : "0"}" = "0" ] || printf '%s\\n' ' M install.sh' ;;
  *) exit 1 ;;
esac
`,
  );

  const env: NodeJS.ProcessEnv = {
    ...process.env,
    BOOTSTRAP_LOG: logPath,
    DOTFILES_DIR: dotfilesDir,
    HOME: homeDir,
    PATH: `${fakeBin}:/usr/bin:/bin`,
    BOOTSTRAP_GIT_LOG: gitLogPath,
    BOOTSTRAP_REVISION: options.revision ?? trustedRevision,
  };

  return {
    env,
    gitLogPath,
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

async function runCommand(command: string, args: string[], cwd: string) {
  const proc = Bun.spawn([command, ...args], {
    cwd,
    stderr: "pipe",
    stdout: "pipe",
  });
  const [exitCode, stderr, stdout] = await Promise.all([
    proc.exited,
    new Response(proc.stderr).text(),
    new Response(proc.stdout).text(),
  ]);
  if (exitCode !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed: ${stderr}`);
  }
  return stdout.trim();
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

  test("新規環境では固定revisionをdetached checkoutしてからbootstrapする", async () => {
    await withTempDir("bootstrap-fresh", async (tempDir) => {
      const { env, gitLogPath, logPath } = await prepareBootstrapEnvironment(tempDir, {
        checkoutExists: false,
      });
      env.DOTFILES_REPO_URL = "https://example.test/dotfiles.git";

      const result = await runScript(installScript, env);

      expect(result).toEqual({ exitCode: 0, stderr: "", stdout: "" });
      expect(await readFile(gitLogPath, "utf8")).toContain(
        `<clone><--no-checkout><https://example.test/dotfiles.git><${env.DOTFILES_DIR}>\n` +
          `<-C><${env.DOTFILES_DIR}><checkout><--detach><${trustedRevision}>\n`,
      );
      expect(await readFile(logPath, "utf8")).toContain("install-mise\n");
    });
  });

  test("実際のgitで指定revisionをcloneしてbootstrapする", async () => {
    await withTempDir("bootstrap-git", async (tempDir) => {
      const sourceDir = path.join(tempDir, "source");
      const dotfilesDir = path.join(tempDir, "checkout");
      const homeDir = path.join(tempDir, "home");
      const logPath = path.join(tempDir, "bootstrap.log");
      await runCommand("git", ["init", "--quiet", sourceDir], tempDir);
      await runCommand("git", ["-C", sourceDir, "config", "user.email", "test@example.invalid"], tempDir);
      await runCommand("git", ["-C", sourceDir, "config", "user.name", "Bootstrap Test"], tempDir);
      await makeExecutable(
        path.join(sourceDir, "bin", "install-mise.sh"),
        `#!/bin/sh
printf 'install-mise\\n' >> "$BOOTSTRAP_LOG"
`,
      );
      await runCommand("git", ["-C", sourceDir, "add", "bin/install-mise.sh"], tempDir);
      await runCommand("git", ["-C", sourceDir, "commit", "--quiet", "-m", "fixture"], tempDir);
      const revision = await runCommand("git", ["-C", sourceDir, "rev-parse", "HEAD"], tempDir);
      await makeExecutable(
        path.join(homeDir, ".local", "bin", "mise"),
        `#!/bin/sh
printf 'mise' >> "$BOOTSTRAP_LOG"
printf ' <%s>' "$@" >> "$BOOTSTRAP_LOG"
printf '\\n' >> "$BOOTSTRAP_LOG"
`,
      );

      const result = await runScript(installScript, {
        ...process.env,
        BOOTSTRAP_LOG: logPath,
        DOTFILES_DIR: dotfilesDir,
        DOTFILES_REPO_URL: sourceDir,
        DOTFILES_REVISION: revision,
        HOME: homeDir,
        PATH: "/usr/bin:/bin",
      });

      expect(result.exitCode).toBe(0);
      expect(await runCommand("git", ["-C", dotfilesDir, "rev-parse", "HEAD"], tempDir)).toBe(revision);
      expect(await readFile(logPath, "utf8")).toBe(
        "install-mise\nmise <trust>\nmise <bootstrap> <--yes>\n",
      );
    });
  });

  test("既定revisionが現在のbootstrap entrypointを含む", async () => {
    await withTempDir("bootstrap-default-revision", async (tempDir) => {
      const dotfilesDir = path.join(tempDir, "checkout");
      const homeDir = path.join(tempDir, "home");
      const logPath = path.join(tempDir, "bootstrap.log");
      await makeExecutable(
        path.join(homeDir, ".local", "bin", "mise"),
        `#!/bin/sh
if [ "\${1:-}" = "--version" ]; then
  printf '%s\n' '2026.7.7 macos-arm64'
  exit 0
fi
printf 'mise' >> "$BOOTSTRAP_LOG"
printf ' <%s>' "$@" >> "$BOOTSTRAP_LOG"
printf '\n' >> "$BOOTSTRAP_LOG"
`,
      );

      const result = await runScript(installScript, {
        ...process.env,
        BOOTSTRAP_LOG: logPath,
        DOTFILES_DIR: dotfilesDir,
        DOTFILES_REPO_URL: repoRoot,
        HOME: homeDir,
        PATH: "/usr/bin:/bin",
      });

      expect(result.exitCode).toBe(0);
      expect(await runCommand("git", ["-C", dotfilesDir, "rev-parse", "HEAD"], tempDir))
        .toBe(trustedRevision);
      expect(await readFile(logPath, "utf8")).toBe(
        "mise <trust>\nmise <bootstrap> <--yes>\n",
      );
    });
  });

  test("既存checkoutのrevisionが異なれば信頼も実行もしない", async () => {
    await withTempDir("bootstrap-revision-mismatch", async (tempDir) => {
      const { env, logPath } = await prepareBootstrapEnvironment(tempDir, {
        revision: "0000000000000000000000000000000000000000",
      });

      const result = await runScript(installScript, env);

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain(`expected dotfiles revision ${trustedRevision}`);
      expect(await Bun.file(logPath).exists()).toBe(false);
    });
  });

  test("固定revisionでもlocal changesがあれば信頼も実行もしない", async () => {
    await withTempDir("bootstrap-dirty", async (tempDir) => {
      const { env, logPath } = await prepareBootstrapEnvironment(tempDir, { dirty: true });

      const result = await runScript(installScript, env);

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("dotfiles checkout has local changes");
      expect(await Bun.file(logPath).exists()).toBe(false);
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
