import { describe, expect, test } from "bun:test";
import { access, chmod, cp, lstat, mkdir, readFile, symlink, writeFile } from "node:fs/promises";
import path from "node:path";
import {
  DotUsageError,
  dotCommands,
  formatDotHelp,
  parseDotCommand,
  pullDotfiles,
} from "../lib/dot";
import { withTempDir, writeTree } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");

describe("dotのコマンド契約", () => {
  test("引数なしと一般的なhelp表記を受け付ける", () => {
    for (const args of [[], ["help"], ["-h"], ["--help"]]) {
      expect(parseDotCommand(args)).toEqual({ type: "help" });
    }
    for (const command of dotCommands) {
      expect(parseDotCommand(["help", command])).toEqual({ command, type: "help" });
      expect(parseDotCommand([command, "help"])).toEqual({ command, type: "help" });
      expect(parseDotCommand([command, "-h"])).toEqual({ command, type: "help" });
      expect(parseDotCommand([command, "--help"])).toEqual({ command, type: "help" });
      expect(parseDotCommand([command])).toEqual({ command, type: "run" });
    }
  });

  test("未知のコマンドと余分な引数を拒否する", () => {
    expect(() => parseDotCommand(["aplpy"])).toThrow(new DotUsageError("unknown command 'aplpy'"));
    expect(() => parseDotCommand(["plan", "extra"])).toThrow(
      new DotUsageError("unexpected arguments for 'plan'"),
    );
  });

  test("ルートとサブコマンドのhelpを表示する", () => {
    expect(formatDotHelp()).toContain("Usage: dot <command>");
    expect(formatDotHelp()).toContain("  pull    ");
    expect(formatDotHelp("apply")).toBe(
      "Usage: dot apply\n\nReview and apply home-directory deployment changes.",
    );
  });

  test("CLIは未知のコマンドを候補なしで引数エラーにする", async () => {
    const result = await runProcess(
      [process.execPath, path.join(repoRoot, "bin/dot.ts"), "aplpy"],
      repoRoot,
    );

    expect(result).toEqual({
      exitCode: 2,
      stderr: "dot: unknown command 'aplpy'\nRun 'dot help' for usage.\n",
      stdout: "",
    });
  });

  test("配備入口は実体からrepositoryを解決してbinのCLIを起動する", async () => {
    await withTempDir("dot-launcher", async (tempDir) => {
      const homeDir = path.join(tempDir, "home");
      const deployedDot = path.join(homeDir, ".local/bin/dot");
      const fakeMise = path.join(homeDir, ".local/bin/mise");
      const fakeBun = path.join(homeDir, ".local/bin/bun");
      await mkdir(path.dirname(deployedDot), { recursive: true });
      await symlink(path.join(repoRoot, "home/.local/bin/dot"), deployedDot);
      await writeFile(fakeMise, [
        "#!/bin/sh",
        "test \"$MISE_AUTO_INSTALL\" = 0",
        "test \"$1\" = which",
        "test \"$2\" = bun",
        `printf '%s\\n' '${fakeBun}'`,
      ].join("\n"));
      await writeFile(fakeBun, [
        "#!/bin/sh",
        "printf 'cwd=%s\\n' \"$PWD\"",
        "printf '<%s>' \"$@\"",
        "printf '\\n'",
      ].join("\n"));
      await chmod(fakeMise, 0o755);
      await chmod(fakeBun, 0o755);

      const result = await runProcess([deployedDot, "--help"], tempDir, {
        ...process.env,
        HOME: homeDir,
      });

      expect(result).toEqual({
        exitCode: 0,
        stderr: "",
        stdout: [
          `cwd=${repoRoot}`,
          `<${path.join(repoRoot, "bin/dot.ts")}><--help>`,
          "",
        ].join("\n"),
      });
    });
  });
});

describe("dot pull", () => {
  test("origin/masterと同一なら変更しない", async () => {
    await withGitRepositories("pull-current", async ({ local }) => {
      await expect(pullDotfiles(local)).resolves.toBe("Dotfiles are up to date.");
    });
  });

  test("origin/masterの変更をfast-forwardする", async () => {
    await withGitRepositories("pull-behind", async ({ local, seed }) => {
      await writeFile(path.join(seed, "managed.txt"), "remote\n");
      await git(seed, ["add", "managed.txt"]);
      await git(seed, ["commit", "-m", "remote change"]);
      await git(seed, ["push", "origin", "master"]);

      const output = await pullDotfiles(local);

      expect(await readFile(path.join(local, "managed.txt"), "utf8")).toBe("remote\n");
      expect(output).toMatch(/^Updated dotfiles: [0-9a-f]{7} → [0-9a-f]{7} \(1 commit\)\.\n/);
      expect(output).toContain("Run 'dot plan' to review deployment changes.");
    });
  });

  test("ローカルが先行していれば変更せず成功する", async () => {
    await withGitRepositories("pull-ahead", async ({ local }) => {
      await writeFile(path.join(local, "local.txt"), "local\n");
      await git(local, ["add", "local.txt"]);
      await git(local, ["commit", "-m", "local change"]);

      await expect(pullDotfiles(local)).resolves.toBe(
        "Dotfiles are 1 commit ahead of origin/master; nothing to pull.",
      );
    });
  });

  test("未コミット変更と分岐を拒否する", async () => {
    await withGitRepositories("pull-refuse", async ({ local, seed }) => {
      await writeFile(path.join(local, "dirty.txt"), "dirty\n");
      await expect(pullDotfiles(local)).rejects.toThrow("pull requires a clean worktree");

      await git(local, ["add", "dirty.txt"]);
      await git(local, ["commit", "-m", "local change"]);
      await writeFile(path.join(seed, "remote.txt"), "remote\n");
      await git(seed, ["add", "remote.txt"]);
      await git(seed, ["commit", "-m", "remote change"]);
      await git(seed, ["push", "origin", "master"]);

      await expect(pullDotfiles(local)).rejects.toThrow(
        "local master and origin/master have diverged",
      );
    });
  });

  test("master以外のbranchを拒否する", async () => {
    await withGitRepositories("pull-branch", async ({ local }) => {
      await git(local, ["switch", "-c", "topic"]);
      await expect(pullDotfiles(local)).rejects.toThrow(
        "pull requires branch 'master'; current checkout is 'topic'",
      );
    });
  });
});

describe("dot applyの確認", () => {
  test("標準入力のyesで表示済み計画を適用する", async () => {
    await withDeploymentFixture("apply-confirm", async ({ homeDir, fixtureRoot }) => {
      const result = await runLinkHome(fixtureRoot, homeDir, "yes\n");

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("link    home/.zshrc → ~/.zshrc");
      expect(result.stdout).toContain("Apply these changes? Type 'yes' to continue:");
      expect(result.stdout).toContain("Applied 1 change.");
      expect(await readFile(path.join(homeDir, ".zshrc"), "utf8")).toBe("managed\n");

      const noChanges = await runLinkHome(fixtureRoot, homeDir, "");
      expect(noChanges.exitCode).toBe(0);
      expect(noChanges.stdout).toBe("1 unchanged\n");
    });
  });

  test("yes以外では変更せず失敗終了する", async () => {
    await withDeploymentFixture("apply-cancel", async ({ homeDir, fixtureRoot }) => {
      const result = await runLinkHome(fixtureRoot, homeDir, "no\n");

      expect(result.exitCode).toBe(1);
      expect(result.stdout).toContain("Apply cancelled.");
      await expect(readFile(path.join(homeDir, ".zshrc"), "utf8")).rejects.toThrow();

      const eof = await runLinkHome(fixtureRoot, homeDir, "");
      expect(eof.exitCode).toBe(1);
      expect(eof.stdout).toContain("Apply cancelled.");
      await expect(readFile(path.join(homeDir, ".zshrc"), "utf8")).rejects.toThrow();
    });
  });

  test("確認キャンセルでは予約したbackup directoryを残さない", async () => {
    await withDeploymentFixture("apply-cancel-backup", async ({ homeDir, fixtureRoot }) => {
      await writeFile(path.join(homeDir, ".zshrc"), "old\n");

      const result = await runLinkHome(fixtureRoot, homeDir, "no\n");

      expect(result.exitCode).toBe(1);
      expect(result.stdout).toContain("Apply cancelled.");
      expect(await readFile(path.join(homeDir, ".zshrc"), "utf8")).toBe("old\n");
      await expect(access(path.join(homeDir, ".dotfiles-backups"))).rejects.toThrow();
    });
  });

  test("確認後の適用で表示したbackup rootへ退避する", async () => {
    await withDeploymentFixture("apply-confirm-backup", async ({ homeDir, fixtureRoot }) => {
      await writeFile(path.join(homeDir, ".zshrc"), "old\n");

      const result = await runLinkHome(fixtureRoot, homeDir, "yes\n");

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Apply these changes? Type 'yes' to continue:");
      expect(result.stdout).toContain("Applied 1 change.");
      const displayedBackup = result.stdout.match(/\.dotfiles-backups\/\S+/)?.[0];
      expect(displayedBackup).toBeDefined();
      expect(await readFile(path.join(homeDir, displayedBackup!), "utf8")).toBe("old\n");
      expect((await lstat(path.join(homeDir, ".zshrc"))).isSymbolicLink()).toBe(true);
    });
  });

  test("absolute entrypoint pathは無関係なCWDから実行しても同じリポジトリを対象にする", async () => {
    await withDeploymentFixture("apply-foreign-cwd", async ({ homeDir, fixtureRoot }) => {
      const elsewhere = path.join(path.dirname(fixtureRoot), "elsewhere");
      await mkdir(elsewhere, { recursive: true });

      const result = await runProcess(
        [process.execPath, path.join(fixtureRoot, "bin/link-home.ts"), "--confirm"],
        elsewhere,
        {
          ...process.env,
          HOME: homeDir,
          XDG_STATE_HOME: path.join(homeDir, ".state"),
        },
        "yes\n",
      );

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Applied 1 change.");
      expect(await readFile(path.join(homeDir, ".zshrc"), "utf8")).toBe("managed\n");
    });
  });
});

async function withGitRepositories(
  name: string,
  run: (repositories: { local: string; seed: string }) => Promise<void>,
) {
  await withTempDir(name, async (tempDir) => {
    const remote = path.join(tempDir, "remote.git");
    const seed = path.join(tempDir, "seed");
    const local = path.join(tempDir, "local");
    await git(tempDir, ["init", "--bare", "--initial-branch=master", remote]);
    await git(tempDir, ["init", "--initial-branch=master", seed]);
    await configureGit(seed);
    await writeFile(path.join(seed, "managed.txt"), "initial\n");
    await git(seed, ["add", "managed.txt"]);
    await git(seed, ["commit", "-m", "initial"]);
    await git(seed, ["remote", "add", "origin", remote]);
    await git(seed, ["push", "-u", "origin", "master"]);
    await git(tempDir, ["clone", "--quiet", remote, local]);
    await configureGit(local);
    await run({ local, seed });
  });
}

async function configureGit(repo: string) {
  await git(repo, ["config", "user.email", "dot-test@example.test"]);
  await git(repo, ["config", "user.name", "Dot Test"]);
}

async function git(cwd: string, args: string[]) {
  const result = await runProcess(["git", ...args], cwd);
  if (result.exitCode !== 0) {
    throw new Error(`git ${args.join(" ")} failed: ${result.stderr}`);
  }
  return result.stdout.trim();
}

async function withDeploymentFixture(
  name: string,
  run: (fixture: { fixtureRoot: string; homeDir: string }) => Promise<void>,
) {
  await withTempDir(name, async (tempDir) => {
    const fixtureRoot = path.join(tempDir, "repo");
    const homeDir = path.join(tempDir, "home-dir");
    await writeTree(fixtureRoot, {
      ".dotfiles.json": JSON.stringify({ symlink: [".zshrc"] }),
      "home/.zshrc": "managed\n",
    });
    await cp(path.join(repoRoot, "bin"), path.join(fixtureRoot, "bin"), { recursive: true });
    await cp(path.join(repoRoot, "lib"), path.join(fixtureRoot, "lib"), { recursive: true });
    await mkdir(homeDir, { recursive: true });
    await run({ fixtureRoot, homeDir });
  });
}

async function runLinkHome(cwd: string, homeDir: string, input: string) {
  return await runProcess(
    [process.execPath, path.join(cwd, "bin/link-home.ts"), "--confirm"],
    cwd,
    {
      ...process.env,
      HOME: homeDir,
      XDG_STATE_HOME: path.join(homeDir, ".state"),
    },
    input,
  );
}

async function runProcess(
  command: string[],
  cwd: string,
  env: NodeJS.ProcessEnv = process.env,
  input?: string,
) {
  const child = Bun.spawn(command, {
    cwd,
    env,
    stderr: "pipe",
    stdin: input === undefined ? "ignore" : "pipe",
    stdout: "pipe",
  });
  if (input !== undefined) {
    const stdin = child.stdin;
    if (stdin === undefined) {
      throw new Error("child stdin is unavailable");
    }
    stdin.write(input);
    stdin.end();
  }
  const [exitCode, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  return { exitCode, stderr, stdout };
}
