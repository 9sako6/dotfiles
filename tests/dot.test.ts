import { describe, expect, test } from "bun:test";
import { access, cp, lstat, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");

describe("applyの確認", () => {
  test("entrypointの引数なしapplyを拒否する", async () => {
    const result = await runProcess(
      [process.execPath, path.join(repoRoot, "bin/link-home.ts")],
      repoRoot,
    );
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("Unknown link-home arguments");
  });

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

  test("標準入力が開いた端末でも確認後に終了する", async () => {
    await withDeploymentFixture("apply-open-stdin", async ({ homeDir, fixtureRoot }) => {
      const result = await runLinkHomeWithOpenInput(fixtureRoot, homeDir, "yes\n");

      expect(result.timedOut).toBe(false);
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Applied 1 change.");
    });
  });

  test("標準入力が開いた端末からCRだけで確定できる", async () => {
    await withDeploymentFixture("apply-open-stdin-cr", async ({ homeDir, fixtureRoot }) => {
      const result = await runLinkHomeWithOpenInput(fixtureRoot, homeDir, "yes\r");

      expect(result.timedOut).toBe(false);
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Applied 1 change.");
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

async function runLinkHomeWithOpenInput(cwd: string, homeDir: string, input: string) {
  const child = Bun.spawn(
    [process.execPath, path.join(cwd, "bin/link-home.ts"), "--confirm"],
    {
      cwd,
      env: {
        ...process.env,
        HOME: homeDir,
        XDG_STATE_HOME: path.join(homeDir, ".state"),
      },
      stderr: "pipe",
      stdin: "pipe",
      stdout: "pipe",
    },
  );
  const stdin = child.stdin;
  if (stdin === undefined) {
    throw new Error("child stdin is unavailable");
  }
  stdin.write(input);

  const outcome = await Promise.race([
    child.exited.then((exitCode) => ({ exitCode, timedOut: false })),
    Bun.sleep(2_000).then(() => ({ exitCode: null, timedOut: true })),
  ]);
  if (outcome.timedOut) {
    child.kill();
  }
  stdin.end();
  const [exitCode, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  return { exitCode, stderr, stdout, timedOut: outcome.timedOut };
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
