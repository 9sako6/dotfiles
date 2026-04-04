import { describe, expect, test } from "bun:test";
import { chmod, copyFile, mkdir, readFile, symlink, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers";

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

async function initRepoWithManagedGitConfig(tempDir: string) {
  const homeDir = path.join(tempDir, "home");
  const repoDir = path.join(tempDir, "repo");
  const hooksDir = path.join(homeDir, ".config", "git", "hooks");
  const globalConfigPath = path.join(homeDir, ".gitconfig");
  const mybinDir = path.join(homeDir, "mybin");
  const undoScriptPath = path.join(mybinDir, "git-undo");
  const env = {
    ...process.env,
    HOME: homeDir,
    GIT_CONFIG_GLOBAL: globalConfigPath,
    GIT_CONFIG_NOSYSTEM: "1",
  };

  await mkdir(hooksDir, { recursive: true });
  await mkdir(repoDir, { recursive: true });
  await mkdir(mybinDir, { recursive: true });
  await copyFile("dist/.gitconfig", globalConfigPath);
  await copyFile("dist/mybin/git-undo", undoScriptPath);
  await chmod(undoScriptPath, 0o755);

  const initResult = await runCommand("git", ["-C", repoDir, "init", "-b", "master"], env);
  expect(initResult.code).toBe(0);

  return { env, repoDir };
}

async function runGit(repoDir: string, env: NodeJS.ProcessEnv, ...args: string[]) {
  return runCommand("git", ["-C", repoDir, ...args], env);
}

async function writeRepoFile(repoDir: string, relativePath: string, content: string) {
  const filePath = path.join(repoDir, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content, "utf8");
}

async function createMinimalZshHome(tempDir: string, options?: { direnvPath?: string | null }) {
  const homeDir = path.join(tempDir, "home");
  const misePath = path.join(homeDir, ".local", "bin", "mise");
  const direnvPath = options?.direnvPath ?? null;

  await writeTree(homeDir, {
    ".zsh.d/prompt.zsh": "export PROMPT_LOADED=1\n",
    ".zsh.d/keybindings.zsh": "export KEYBINDINGS_LOADED=1\n",
    ".zsh.d/functions.zsh": "export FUNCTIONS_LOADED=1\n",
    ".zsh.d/local.zsh": "export LOCAL_LOADED=1\n",
  });
  await writeTree(path.dirname(misePath), {
    mise: `#!/bin/sh
set -eu
case "$1" in
  activate)
    exit 0
    ;;
  which)
    case "$2" in
      direnv)
        if [ -n "${direnvPath ?? ""}" ]; then
          printf '%s\n' "${direnvPath ?? ""}"
          exit 0
        fi
        exit 1
        ;;
      *)
        exit 1
        ;;
    esac
    ;;
  *)
    exit 1
    ;;
esac
`,
  });
  await chmod(misePath, 0o755);

  return { homeDir };
}

describe("shell config", () => {
  test("zshenv sources secrets before interactive shell config loads", async () => {
    await withTempDir("zshenv-secrets", async (tempDir) => {
      const homeDir = path.join(tempDir, "home");

      await writeTree(homeDir, {
        ".zsh.d/secrets.zsh": "export SECRET_FROM_TEST=loaded\n",
      });

      const result = await runCommand("zsh", ["-f", "-c", "source dist/.zshenv; printf '%s' \"$SECRET_FROM_TEST\""], {
        ...process.env,
        HOME: homeDir,
      });

      expect(result.code).toBe(0);
      expect(result.stdout).toBe("loaded");
    });
  });

  test("zshrc loads tracked fragments and repo aliases into an interactive shell", async () => {
    await withTempDir("zshrc-fragments", async (tempDir) => {
      const { homeDir } = await createMinimalZshHome(tempDir);

      await copyFile("dist/.zsh.d/alias.zsh", path.join(homeDir, ".zsh.d", "alias.zsh"));

      const result = await runCommand(
        "zsh",
        [
          "-f",
          "-i",
          "-c",
          "abbrev-alias(){ alias \"$@\"; }; source dist/.zshrc; alias codex; printf ' prompt=%s key=%s fn=%s local=%s' \"$PROMPT_LOADED\" \"$KEYBINDINGS_LOADED\" \"$FUNCTIONS_LOADED\" \"$LOCAL_LOADED\"",
        ],
        {
          ...process.env,
          DOTFILES_NO_BANNER: "1",
          HOME: homeDir,
          PATH: `${path.join(homeDir, ".local", "bin")}:${process.env.PATH ?? ""}`,
        },
      );

      expect(result.code).toBe(0);
      expect(result.stdout).toContain("codex='command codex --no-alt-screen'");
      expect(result.stdout).toContain("prompt=1 key=1 fn=1 local=1");
    });
  });

  test("zshrc only enables direnv when mise can resolve it", async () => {
    await withTempDir("zshrc-direnv", async (tempDir) => {
      const binDir = path.join(tempDir, "bin");
      const direnvPath = path.join(binDir, "direnv");
      const { homeDir } = await createMinimalZshHome(tempDir, { direnvPath });

      await writeTree(binDir, {
        direnv: `#!/bin/sh
printf '%s\n' 'export DIRENV_HOOK_LOADED=1'
`,
      });
      await copyFile("dist/.zsh.d/alias.zsh", path.join(homeDir, ".zsh.d", "alias.zsh"));
      await chmod(direnvPath, 0o755);

      const result = await runCommand(
        "zsh",
        [
          "-f",
          "-i",
          "-c",
          "abbrev-alias(){ alias \"$@\"; }; source dist/.zshrc; printf '%s' \"$DIRENV_HOOK_LOADED\"",
        ],
        {
          ...process.env,
          DOTFILES_NO_BANNER: "1",
          HOME: homeDir,
          PATH: `${path.join(homeDir, ".local", "bin")}:${binDir}:${process.env.PATH ?? ""}`,
        },
      );

      expect(result.code).toBe(0);
      expect(result.stdout).toBe("1");
    });
  });

  test("pre-commit hook warns when gitleaks is unavailable but does not block commits", async () => {
    await withTempDir("pre-commit-warning", async (tempDir) => {
      const repoDir = path.join(tempDir, "repo");
      const hookPath = path.join(process.cwd(), "dist/.config/git/hooks/pre-commit");
      const gitBinDirResult = await runCommand("/bin/sh", ["-c", "command -v git"]);

      expect(gitBinDirResult.code).toBe(0);
      const gitBin = gitBinDirResult.stdout.trim();
      expect(gitBin).not.toBe("");

      await mkdir(repoDir, { recursive: true });
      expect((await runCommand("git", ["init", "-b", "master"], process.env, { cwd: repoDir })).code).toBe(0);

      const hookResult = await runCommand(
        "/bin/sh",
        [hookPath],
        { ...process.env, PATH: path.dirname(gitBin) },
        { cwd: repoDir },
      );

      expect(hookResult.code).toBe(0);
      expect(hookResult.stderr).toContain("gitleaks: not installed, skipping local secret scan.");
    });
  });

  test("git undo unstages staged changes before touching commits", async () => {
    await withTempDir("git-undo-unstage", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);

      await writeRepoFile(repoDir, "note.txt", "before\n");
      expect((await runGit(repoDir, env, "add", "note.txt")).code).toBe(0);
      expect((await runGit(repoDir, env, "commit", "-m", "init")).code).toBe(0);

      await writeRepoFile(repoDir, "note.txt", "after\n");
      expect((await runGit(repoDir, env, "add", "note.txt")).code).toBe(0);

      const undoResult = await runGit(repoDir, env, "undo");
      expect(undoResult.code).toBe(0);

      const stagedNames = await runGit(repoDir, env, "diff", "--cached", "--name-only");
      const unstagedNames = await runGit(repoDir, env, "diff", "--name-only");
      expect(stagedNames.stdout).toBe("");
      expect(unstagedNames.stdout).toBe("note.txt\n");
    });
  });

  test("git undo moves the latest commit back to staged changes", async () => {
    await withTempDir("git-undo-commit", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);

      await writeRepoFile(repoDir, "note.txt", "one\n");
      expect((await runGit(repoDir, env, "add", "note.txt")).code).toBe(0);
      expect((await runGit(repoDir, env, "commit", "-m", "init")).code).toBe(0);

      await writeRepoFile(repoDir, "note.txt", "two\n");
      expect((await runGit(repoDir, env, "add", "note.txt")).code).toBe(0);
      expect((await runGit(repoDir, env, "commit", "-m", "second")).code).toBe(0);

      const undoResult = await runGit(repoDir, env, "undo");
      expect(undoResult.code).toBe(0);

      const commitCount = await runGit(repoDir, env, "rev-list", "--count", "HEAD");
      const latestSubject = await runGit(repoDir, env, "log", "-1", "--pretty=%s");
      const stagedNames = await runGit(repoDir, env, "diff", "--cached", "--name-only");
      expect(commitCount.stdout).toBe("1\n");
      expect(latestSubject.stdout).toBe("init\n");
      expect(stagedNames.stdout).toBe("note.txt\n");
    });
  });

  test("git undo can uncommit the initial commit back to staged changes", async () => {
    await withTempDir("git-undo-root", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);

      await writeRepoFile(repoDir, "note.txt", "root\n");
      expect((await runGit(repoDir, env, "add", "note.txt")).code).toBe(0);
      expect((await runGit(repoDir, env, "commit", "-m", "root")).code).toBe(0);

      const undoResult = await runGit(repoDir, env, "undo");
      expect(undoResult.code).toBe(0);

      const headResult = await runGit(repoDir, env, "rev-parse", "--verify", "HEAD");
      const statusResult = await runGit(repoDir, env, "status", "--short");
      expect(headResult.code).not.toBe(0);
      expect(statusResult.stdout).toBe("A  note.txt\n");
    });
  });

  test("git undo path only unstages the requested path", async () => {
    await withTempDir("git-undo-path", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);

      await writeRepoFile(repoDir, "a.txt", "one\n");
      await writeRepoFile(repoDir, "b.txt", "one\n");
      expect((await runGit(repoDir, env, "add", "a.txt", "b.txt")).code).toBe(0);
      expect((await runGit(repoDir, env, "commit", "-m", "init")).code).toBe(0);

      await writeRepoFile(repoDir, "a.txt", "two\n");
      await writeRepoFile(repoDir, "b.txt", "two\n");
      expect((await runGit(repoDir, env, "add", "a.txt", "b.txt")).code).toBe(0);

      const undoResult = await runGit(repoDir, env, "undo", "a.txt");
      expect(undoResult.code).toBe(0);

      const stagedNames = await runGit(repoDir, env, "diff", "--cached", "--name-only");
      const unstagedNames = await runGit(repoDir, env, "diff", "--name-only");
      expect(stagedNames.stdout).toBe("b.txt\n");
      expect(unstagedNames.stdout).toBe("a.txt\n");
    });
  });

  test("git undo path fails when the requested path is not staged", async () => {
    await withTempDir("git-undo-path-error", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);

      await writeRepoFile(repoDir, "note.txt", "one\n");
      expect((await runGit(repoDir, env, "add", "note.txt")).code).toBe(0);
      expect((await runGit(repoDir, env, "commit", "-m", "init")).code).toBe(0);

      await writeRepoFile(repoDir, "note.txt", "two\n");

      const undoResult = await runGit(repoDir, env, "undo", "note.txt");
      expect(undoResult.code).toBe(1);
      expect(undoResult.stderr).toContain("git undo: no staged changes for: note.txt");
    });
  });

  test("git undo fails when there is nothing to undo", async () => {
    await withTempDir("git-undo-empty", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);

      const undoResult = await runGit(repoDir, env, "undo");
      expect(undoResult.code).toBe(1);
      expect(undoResult.stderr).toContain("git undo: nothing to undo");
    });
  });

  test("nyanpasu passes layout as a global zellij option", async () => {
    await withTempDir("nyanpasu", async (tempDir) => {
      const binDir = path.join(tempDir, "bin");
      const capturePath = path.join(tempDir, "zellij-args");

      await writeTree(binDir, {
        zellij: `#!/bin/sh
printf '%s\n' "$@" > "${capturePath}"
`,
      });
      await chmod(path.join(binDir, "zellij"), 0o755);

      const result = await runCommand("sh", ["dist/mybin/nyanpasu"], {
        ...process.env,
        ZELLIJ: "0",
        PATH: `${binDir}:${process.env.PATH ?? ""}`,
      });

      expect(result.code).toBe(0);
      expect(await readFile(capturePath, "utf8")).toBe("--layout\nquad\nattach\nnyanpasu\n-c\n");
    });
  });

  test("tada stays quiet and exits successfully on unsupported environments", async () => {
    const result = await runCommand("sh", ["dist/mybin/tada"], {
      ...process.env,
      TADA_UNAME: "Linux",
    });

    expect(result.code).toBe(0);
    expect(result.stdout).toBe("");
    expect(result.stderr).toBe("");
  });

  test("tada launches the bundled binary on macOS", async () => {
    await withTempDir("tada", async (tempDir) => {
      const binDir = path.join(tempDir, "bin");
      const launchCapturePath = path.join(tempDir, "launched");
      const bundledBinaryPath = path.join(tempDir, "tada-darwin-arm64");

      await writeTree(binDir, {
        nohup: `#!/bin/sh
"$@"
`,
      });

      await writeTree(tempDir, {
        "tada-darwin-arm64": `#!/bin/sh
printf 'ok\n' > "${launchCapturePath}"
`,
      });

      await Promise.all([
        chmod(path.join(binDir, "nohup"), 0o755),
        chmod(bundledBinaryPath, 0o755),
      ]);

      const result = await runCommand("sh", ["dist/mybin/tada"], {
        ...process.env,
        PATH: `${binDir}:${process.env.PATH ?? ""}`,
        TADA_NOHUP_BIN: path.join(binDir, "nohup"),
        TADA_BIN_PATH: bundledBinaryPath,
        TADA_UNAME: "Darwin",
      });

      expect(result.code).toBe(0);
      for (let attempts = 0; attempts < 50; attempts += 1) {
        try {
          expect(await readFile(launchCapturePath, "utf8")).toContain("ok");
          return;
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
            throw error;
          }
          await Bun.sleep(20);
        }
      }
      expect(await readFile(launchCapturePath, "utf8")).toContain("ok");
    });
  });

  test("tada resolves a symlinked launcher before locating the bundled binary", async () => {
    await withTempDir("tada", async (tempDir) => {
      const binDir = path.join(tempDir, "bin");
      const realLauncherDir = path.join(tempDir, "dist", "mybin");
      const symlinkLauncherDir = path.join(tempDir, "home", "mybin");
      const launchCapturePath = path.join(tempDir, "launched");
      const realLauncherPath = path.join(realLauncherDir, "tada");
      const symlinkLauncherPath = path.join(symlinkLauncherDir, "tada");
      const bundledBinaryPath = path.join(realLauncherDir, "lib", "tada-darwin-arm64");

      await writeTree(binDir, {
        nohup: `#!/bin/sh
"$@"
`,
      });
      await Promise.all([
        mkdir(path.join(realLauncherDir, "lib"), { recursive: true }),
        mkdir(symlinkLauncherDir, { recursive: true }),
      ]);
      await copyFile("dist/mybin/tada", realLauncherPath);
      await writeTree(realLauncherDir, {
        "lib/tada-darwin-arm64": `#!/bin/sh
printf 'ok\n' > "${launchCapturePath}"
`,
      });
      await symlink(realLauncherPath, symlinkLauncherPath);
      await Promise.all([
        chmod(path.join(binDir, "nohup"), 0o755),
        chmod(realLauncherPath, 0o755),
        chmod(bundledBinaryPath, 0o755),
      ]);

      const result = await runCommand("sh", [symlinkLauncherPath], {
        ...process.env,
        PATH: `${binDir}:${process.env.PATH ?? ""}`,
        TADA_NOHUP_BIN: path.join(binDir, "nohup"),
        TADA_UNAME: "Darwin",
      });

      expect(result.code).toBe(0);
      for (let attempts = 0; attempts < 50; attempts += 1) {
        try {
          expect(await readFile(launchCapturePath, "utf8")).toContain("ok");
          return;
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
            throw error;
          }
          await Bun.sleep(20);
        }
      }
      expect(await readFile(launchCapturePath, "utf8")).toContain("ok");
    });
  });
});
