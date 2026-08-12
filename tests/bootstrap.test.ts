import { describe, expect, test } from "bun:test";
import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");
const installScript = path.join(repoRoot, "install.sh");
const remoteRevision = "1111111111111111111111111111111111111111";

async function makeExecutable(filePath: string, content: string) {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
  await chmod(filePath, 0o755);
}

function cargoApplyLog(dotfilesDir: string): string {
  return "mise <exec> <--> <cargo> <run> <--locked> <--manifest-path> " +
    `<${dotfilesDir}/cli/Cargo.toml> <--> <apply>\n`;
}

async function prepareBootstrapEnvironment(
  tempDir: string,
  options: {
    ancestor?: boolean;
    branch?: string;
    checkoutExists?: boolean;
    dirty?: boolean;
  } = {},
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
if [ "\${BOOTSTRAP_FAIL_STAGE:-}" = "install-mise" ] && [ ! -e "\${BOOTSTRAP_FAILURE_MARKER:-/nonexistent}" ]; then
  : > "$BOOTSTRAP_FAILURE_MARKER"
  exit 1
fi
`,
  );
  await makeExecutable(
    path.join(homeDir, ".local/bin/mise"),
    `#!/bin/sh
printf 'mise' >> "$BOOTSTRAP_LOG"
printf ' <%s>' "$@" >> "$BOOTSTRAP_LOG"
printf '\\n' >> "$BOOTSTRAP_LOG"
if [ "\${BOOTSTRAP_FAIL_STAGE:-}" = "\${1:-}" ] && [ ! -e "\${BOOTSTRAP_FAILURE_MARKER:-/nonexistent}" ]; then
  : > "$BOOTSTRAP_FAILURE_MARKER"
  exit 1
fi
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
  branch) exit 0 ;;
  checkout) exit 0 ;;
  fetch) exit 0 ;;
  merge-base) [ "${options.ancestor === false ? "0" : "1"}" = "1" ] ;;
  rev-parse) printf '%s\\n' "$BOOTSTRAP_REMOTE_REVISION" ;;
  status) [ "${options.dirty ? "1" : "0"}" = "0" ] || printf '%s\\n' ' M install.sh' ;;
  symbolic-ref)
    [ -n "$BOOTSTRAP_BRANCH" ] || exit 1
    printf '%s\\n' "$BOOTSTRAP_BRANCH"
    ;;
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
    BOOTSTRAP_BRANCH: options.branch ?? "master",
    BOOTSTRAP_REMOTE_REVISION: remoteRevision,
  };

  return { env, gitLogPath, logPath };
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
  test("applyをmise taskではなくRust CLIから実行する", async () => {
    await withTempDir("bootstrap-rust-apply", async (tempDir) => {
      const { env, logPath } = await prepareBootstrapEnvironment(tempDir);

      const result = await runScript(installScript, env);

      expect(result).toEqual({ exitCode: 0, stderr: "", stdout: "" });
      expect(await readFile(logPath, "utf8")).toBe(
        "install-mise\nmise <trust>\nmise <install>\n" +
          cargoApplyLog(env.DOTFILES_DIR!) +
          "mise <bootstrap> <--yes> <--verbose>\n",
      );
    });
  });

  test("新規環境ではorigin/masterの先端をbootstrapしてmasterへ接続する", async () => {
    await withTempDir("bootstrap-fresh", async (tempDir) => {
      const { env, gitLogPath } = await prepareBootstrapEnvironment(tempDir, {
        checkoutExists: false,
      });
      env.DOTFILES_REPO_URL = "https://example.test/dotfiles.git";

      const result = await runScript(installScript, env);

      expect(result).toEqual({ exitCode: 0, stderr: "", stdout: "" });
      expect(await readFile(gitLogPath, "utf8")).toContain(
        `<clone><--no-checkout><https://example.test/dotfiles.git><${env.DOTFILES_DIR}>\n` +
          `<-C><${env.DOTFILES_DIR}><rev-parse><refs/remotes/origin/master>\n` +
          `<-C><${env.DOTFILES_DIR}><checkout><--quiet><--detach><${remoteRevision}>\n`,
      );
      expect(await readFile(gitLogPath, "utf8")).toContain(
        `<-C><${env.DOTFILES_DIR}><checkout><--quiet><-B><master><${remoteRevision}>\n` +
          `<-C><${env.DOTFILES_DIR}><branch><--quiet><--set-upstream-to=origin/master><master>\n`,
      );
    });
  });

  test("実際のgitでもorigin/masterへ収束する", async () => {
    await withTempDir("bootstrap-git", async (tempDir) => {
      const sourceDir = path.join(tempDir, "source");
      const dotfilesDir = path.join(tempDir, "checkout");
      const homeDir = path.join(tempDir, "home");
      const logPath = path.join(tempDir, "bootstrap.log");
      await runCommand("git", ["init", "--quiet", "--initial-branch=master", sourceDir], tempDir);
      await runCommand("git", ["-C", sourceDir, "config", "user.email", "test@example.invalid"], tempDir);
      await runCommand("git", ["-C", sourceDir, "config", "user.name", "Bootstrap Test"], tempDir);
      await makeExecutable(
        path.join(sourceDir, "bin", "install-mise.sh"),
        "#!/bin/sh\nprintf 'install-mise\\n' >> \"$BOOTSTRAP_LOG\"\n",
      );
      await runCommand("git", ["-C", sourceDir, "add", "bin/install-mise.sh"], tempDir);
      await runCommand("git", ["-C", sourceDir, "commit", "--quiet", "-m", "fixture"], tempDir);
      const revision = await runCommand("git", ["-C", sourceDir, "rev-parse", "HEAD"], tempDir);
      await makeExecutable(
        path.join(homeDir, ".local/bin/mise"),
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
        HOME: homeDir,
        PATH: "/usr/bin:/bin",
      });

      expect(result.exitCode).toBe(0);
      expect(await runCommand("git", ["-C", dotfilesDir, "rev-parse", "HEAD"], tempDir)).toBe(revision);
      expect(await runCommand("git", ["-C", dotfilesDir, "branch", "--show-current"], tempDir)).toBe("master");
      expect(await runCommand(
        "git",
        ["-C", dotfilesDir, "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
        tempDir,
      )).toBe("origin/master");
      expect(await readFile(logPath, "utf8")).toContain(cargoApplyLog(dotfilesDir));
    });
  });

  for (const failureStage of ["install-mise", "trust", "exec", "bootstrap"] as const) {
    test(`${failureStage}の失敗後も再実行でmasterへ収束する`, async () => {
      await withTempDir(`bootstrap-retry-${failureStage}`, async (tempDir) => {
        const { env } = await prepareBootstrapEnvironment(tempDir);
        const failureMarker = path.join(tempDir, "failed-once");
        env.BOOTSTRAP_FAILURE_MARKER = failureMarker;
        env.BOOTSTRAP_FAIL_STAGE = failureStage;

        const failed = await runScript(installScript, env);
        expect(failed.exitCode).not.toBe(0);

        const retried = await runScript(installScript, env);
        expect(retried).toEqual({ exitCode: 0, stderr: "", stdout: "" });
      });
    });
  }

  test("既存checkoutがorigin/masterから分岐していれば信頼も実行もしない", async () => {
    await withTempDir("bootstrap-diverged", async (tempDir) => {
      const { env, logPath } = await prepareBootstrapEnvironment(tempDir, { ancestor: false });

      const result = await runScript(installScript, env);

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("dotfiles checkout has commits outside origin/master");
      expect(await Bun.file(logPath).exists()).toBe(false);
    });
  });

  test("既存checkoutにlocal changesがあれば信頼も実行もしない", async () => {
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
