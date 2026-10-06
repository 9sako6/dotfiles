import { describe, expect, test } from "bun:test";
import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");
const installScript = path.join(repoRoot, "install.sh");
const remoteRevision = "1111111111111111111111111111111111111111";
const fixtureGitEnvironment = {
  GIT_CONFIG_GLOBAL: "/dev/null",
  GIT_CONFIG_NOSYSTEM: "1",
  GIT_CONFIG_COUNT: "1",
  GIT_CONFIG_KEY_0: "maintenance.auto",
  GIT_CONFIG_VALUE_0: "false",
};

async function makeExecutable(filePath: string, content: string) {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
  await chmod(filePath, 0o755);
}

const applyLog = "mise <exec> <--> <mise> <run> <home:apply>\n" +
  "mise <exec> <--> <mise> <run> <agents:apply>\n" +
  "mise <exec> <--> <mise> <run> <system:apply>\n";

async function prepareBootstrapEnvironment(
  tempDir: string,
  options: {
    ancestor?: boolean;
    dirty?: boolean;
  } = {},
) {
  const dotfilesDir = path.join(tempDir, "dotfiles");
  const fakeBin = path.join(tempDir, "bin");
  const homeDir = path.join(tempDir, "home");
  const logPath = path.join(tempDir, "bootstrap.log");

  await mkdir(path.join(dotfilesDir, ".git"), { recursive: true });
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
if [ "\${1:-}" = install ] && [ "\${2:-}" = --locked ] && [ "\${3:-}" = bun ]; then
  [ "\${MISE_CONFIG_FILE:-}" = "$DOTFILES_DIR/home/.config/mise/config.toml" ] || exit 1
  : > "$HOME/bun-ready"
fi
if [ "\${1:-}" = exec ]; then
  [ -e "$HOME/bun-ready" ] || exit 1
fi
`,
  );
  await makeExecutable(
    path.join(fakeBin, "git"),
    `#!/bin/sh
set -eu
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
    BOOTSTRAP_BRANCH: "master",
    BOOTSTRAP_REMOTE_REVISION: remoteRevision,
  };

  return { env, logPath };
}

async function runScript(script: string, env: NodeJS.ProcessEnv) {
  const proc = Bun.spawn(["/bin/sh", script], {
    cwd: repoRoot,
    env: { ...env, ...fixtureGitEnvironment },
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

async function runGit(args: string[], cwd: string) {
  const proc = Bun.spawn(["git", ...args], {
    cwd,
    env: { ...process.env, ...fixtureGitEnvironment },
    stderr: "pipe",
    stdout: "pipe",
  });
  const [exitCode, stderr, stdout] = await Promise.all([
    proc.exited,
    new Response(proc.stderr).text(),
    new Response(proc.stdout).text(),
  ]);
  if (exitCode !== 0) {
    throw new Error(`git ${args.join(" ")} failed: ${stderr}`);
  }
  return stdout.trim();
}

describe("公開bootstrap", () => {
  test("固定Bunでhome、agents、systemを順に反映する", async () => {
    await withTempDir("bootstrap-nix-apply", async (tempDir) => {
      const { env, logPath } = await prepareBootstrapEnvironment(tempDir);

      const result = await runScript(installScript, env);

      expect(result).toEqual({ exitCode: 0, stderr: "", stdout: "" });
      expect(await readFile(logPath, "utf8")).toBe(
        "install-mise\n" +
          `mise <trust> <${env.DOTFILES_DIR}/home/.config/mise/config.toml>\nmise <install> <--locked> <bun>\n` +
          "mise <trust>\n" + applyLog,
      );
    });
  });

  test("実際のgitで途中失敗後もorigin/masterへ収束する", async () => {
    await withTempDir("bootstrap-git", async (tempDir) => {
      const sourceDir = path.join(tempDir, "source");
      const dotfilesDir = path.join(tempDir, "checkout");
      const homeDir = path.join(tempDir, "home");
      const logPath = path.join(tempDir, "bootstrap.log");
      await runGit(["init", "--quiet", "--initial-branch=master", sourceDir], tempDir);
      await runGit(["-C", sourceDir, "config", "user.email", "test@example.invalid"], tempDir);
      await runGit(["-C", sourceDir, "config", "user.name", "Bootstrap Test"], tempDir);
      await makeExecutable(
        path.join(sourceDir, "bin", "install-mise.sh"),
        "#!/bin/sh\nprintf 'install-mise\\n' >> \"$BOOTSTRAP_LOG\"\n",
      );
      await runGit(["-C", sourceDir, "add", "bin/install-mise.sh"], tempDir);
      await runGit(["-C", sourceDir, "commit", "--quiet", "-m", "fixture"], tempDir);
      const revision = await runGit(["-C", sourceDir, "rev-parse", "HEAD"], tempDir);
      await makeExecutable(
        path.join(homeDir, ".local/bin/mise"),
        `#!/bin/sh
if [ "$1" = exec ] && [ ! -e "$HOME/failed-once" ]; then
  : > "$HOME/failed-once"
  exit 1
fi
printf 'mise' >> "$BOOTSTRAP_LOG"
printf ' <%s>' "$@" >> "$BOOTSTRAP_LOG"
printf '\\n' >> "$BOOTSTRAP_LOG"
`,
      );
      const env = {
        ...process.env,
        BOOTSTRAP_LOG: logPath,
        DOTFILES_DIR: dotfilesDir,
        DOTFILES_REPO_URL: sourceDir,
        HOME: homeDir,
      };

      const failed = await runScript(installScript, env);
      expect(failed.exitCode).not.toBe(0);
      expect(await runGit(["-C", dotfilesDir, "branch", "--show-current"], tempDir)).toBe("");
      const result = await runScript(installScript, env);

      expect(result.exitCode).toBe(0);
      expect(await runGit(["-C", dotfilesDir, "rev-parse", "HEAD"], tempDir)).toBe(revision);
      expect(await runGit(["-C", dotfilesDir, "branch", "--show-current"], tempDir)).toBe("master");
      expect(await runGit(
        ["-C", dotfilesDir, "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
        tempDir,
      )).toBe("origin/master");
      expect(await readFile(logPath, "utf8")).toContain(applyLog);
    });
  });

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
