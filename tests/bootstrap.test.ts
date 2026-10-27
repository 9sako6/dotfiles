import { describe, expect, test } from "bun:test";
import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");
const installScript = path.join(repoRoot, "install.sh");
const fixtureGitEnvironment = {
  GIT_CONFIG_GLOBAL: "/dev/null",
  GIT_CONFIG_NOSYSTEM: "1",
  GIT_CONFIG_COUNT: "1",
  GIT_CONFIG_KEY_0: "maintenance.auto",
  GIT_CONFIG_VALUE_0: "false",
};

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
  const proc = Bun.spawn(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", ...args], {
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
  if (exitCode !== 0) throw new Error(`git ${args.join(" ")} failed: ${stderr}`);
  return stdout.trim();
}

async function prepareBootstrapEnvironment(root: string) {
  const source = path.join(root, "source");
  const checkout = path.join(root, "checkout");
  const home = path.join(root, "home");
  await mkdir(home);
  await runGit(["init", "--quiet", "--initial-branch=master", source], root);
  await writeTree(source, {
    "bin/install-mise.sh": '#!/bin/sh\nprintf installed > "$HOME/installer-ran"\n',
  });
  await chmod(path.join(source, "bin/install-mise.sh"), 0o755);
  await runGit(["-C", source, "add", "."], root);
  await runGit(["-C", source, "commit", "--quiet", "-m", "fixture"], root);
  const revision = await runGit(["-C", source, "rev-parse", "HEAD"], root);
  await writeTree(home, {
    ".local/bin/mise": '#!/bin/sh\nif [ "$1" = exec ]; then\n  if [ "${FAIL_ONCE:-}" = 1 ] && [ ! -e "$HOME/failed-once" ]; then\n    touch "$HOME/failed-once"\n    exit 1\n  fi\n  printf applied > "$HOME/applied"\nfi\n',
  });
  await chmod(path.join(home, ".local/bin/mise"), 0o755);
  const env = { ...process.env, DOTFILES_DIR: checkout, DOTFILES_REPO_URL: source, HOME: home };
  return { checkout, env, home, revision, source };
}

describe("公開bootstrap", () => {
  test("途中失敗後もorigin/masterへ収束する", async () => {
    await withTempDir("bootstrap-retry", async root => {
      const { checkout, env, home, revision } = await prepareBootstrapEnvironment(root);
      expect((await runScript(installScript, { ...env, FAIL_ONCE: "1" })).exitCode).not.toBe(0);
      expect(await runGit(["-C", checkout, "branch", "--show-current"], root)).toBe("");
      expect((await runScript(installScript, env)).exitCode).toBe(0);
      expect(await runGit(["-C", checkout, "rev-parse", "HEAD"], root)).toBe(revision);
      expect(await runGit(["-C", checkout, "branch", "--show-current"], root)).toBe("master");
      expect(await runGit(["-C", checkout, "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"], root)).toBe("origin/master");
      expect(await readFile(path.join(home, "applied"), "utf8")).toBe("applied");
    });
  });

  test("分岐したcheckoutは保持し、installerを実行しない", async () => {
    await withTempDir("bootstrap-diverged", async root => {
      const { checkout, env, home, source } = await prepareBootstrapEnvironment(root);
      await runGit(["clone", "--quiet", source, checkout], root);
      await writeTree(checkout, { "local": "keep" });
      await runGit(["-C", checkout, "add", "local"], root);
      await runGit(["-C", checkout, "commit", "--quiet", "-m", "local"], root);
      const revision = await runGit(["-C", checkout, "rev-parse", "HEAD"], root);
      expect((await runScript(installScript, env)).exitCode).not.toBe(0);
      expect(await runGit(["-C", checkout, "rev-parse", "HEAD"], root)).toBe(revision);
      expect(await readFile(path.join(checkout, "local"), "utf8")).toBe("keep");
      expect(await Bun.file(path.join(home, "installer-ran")).exists()).toBe(false);
    });
  });

  test("未コミット変更を保持し、installerを実行しない", async () => {
    await withTempDir("bootstrap-dirty", async root => {
      const { checkout, env, home, revision, source } = await prepareBootstrapEnvironment(root);
      await runGit(["clone", "--quiet", source, checkout], root);
      await writeFile(path.join(checkout, "bin/install-mise.sh"), "local changes");
      expect((await runScript(installScript, env)).exitCode).not.toBe(0);
      expect(await readFile(path.join(checkout, "bin/install-mise.sh"), "utf8")).toBe("local changes");
      expect(await runGit(["-C", checkout, "rev-parse", "HEAD"], root)).toBe(revision);
      expect(await Bun.file(path.join(home, "installer-ran")).exists()).toBe(false);
    });
  });

  test("途切れたinstall.shはcheckoutもinstallerも作らない", async () => {
    await withTempDir("bootstrap-truncated", async root => {
      const { checkout, env, home } = await prepareBootstrapEnvironment(root);
      const script = await readFile(installScript, "utf8");
      const truncated = path.join(root, "install-truncated.sh");
      await writeFile(truncated, script.slice(0, script.lastIndexOf("\n", script.length - 2) + 1));
      expect(await runScript(truncated, env)).toEqual({ exitCode: 0, stderr: "", stdout: "" });
      expect(await Bun.file(path.join(checkout, ".git/HEAD")).exists()).toBe(false);
      expect(await Bun.file(path.join(home, "installer-ran")).exists()).toBe(false);
    });
  });
});
