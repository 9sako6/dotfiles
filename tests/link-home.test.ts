import { describe, expect, test } from "bun:test";
import { access, lstat, mkdir, readFile, realpath, rm, unlink, writeFile } from "node:fs/promises";
import path from "node:path";
import { createSymlink, readSymlinkTarget, withTempDir, writeTree } from "./test-helpers";
import { loadDotfilesConfig } from "../scripts/lib/dotfiles-config";
import {
  formatPlan,
  planLinkActions as planTrackedLinkActions,
  runLinkPlan,
  type LinkPlan,
} from "../scripts/lib/link-home";

type PlanOptions = Parameters<typeof planTrackedLinkActions>[0];

function planLinkActions(options: Omit<PlanOptions, "statePath"> & { statePath?: string }) {
  return planTrackedLinkActions({
    ...options,
    statePath: options.statePath ?? path.join(options.homeDir, ".local", "state", "dotfiles", "deployment.json"),
  });
}

describe("配備計画の実行", () => {
  test("リンク先が一致するシンボリックリンクは変更しない", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      const sourcePath = path.join(sourceRoot, ".zshrc");
      const destinationPath = path.join(homeDir, ".zshrc");
      await writeTree(sourceRoot, {
        ".zshrc": "export TEST=1\n",
      });
      await createSymlink(sourcePath, destinationPath);

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        symlinkPaths: new Set([".zshrc"]),
        timestamp: "20260325T120000",
      });
      await runLinkPlan(plan);

      expect(await readSymlinkTarget(destinationPath)).toBe(await realpath(sourcePath));
      await expect(access(path.join(homeDir, ".dotfiles-backups", "20260325T120000", ".zshrc"))).rejects.toThrow();
    });
  });

  test("親ディレクトリを作り、ファイルごとにシンボリックリンクを張る", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".config/mise/config.toml": "tasks = {}\n",
      });
      await mkdir(homeDir, { recursive: true });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        symlinkPaths: new Set([".config/mise"]),
      });
      await runLinkPlan(plan);

      const linkedFile = path.join(homeDir, ".config", "mise", "config.toml");
      await expect(access(linkedFile)).resolves.toBeNull();
      expect(await readSymlinkTarget(linkedFile)).toBe(await realpath(path.join(sourceRoot, ".config", "mise", "config.toml")));
    });
  });

  test("競合するファイルをバックアップ先へ移す", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".zshrc": "new\n",
      });
      await writeTree(homeDir, {
        ".zshrc": "old\n",
      });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        symlinkPaths: new Set([".zshrc"]),
        timestamp: "20260325T120000",
      });
      await runLinkPlan(plan);

      const backupPath = path.join(homeDir, ".dotfiles-backups", "20260325T120000", ".zshrc");
      expect(await readFile(backupPath, "utf8")).toBe("old\n");
      expect(await readSymlinkTarget(path.join(homeDir, ".zshrc"))).toBe(await realpath(path.join(sourceRoot, ".zshrc")));
    });
  });

  test("dry-runではファイルシステムを変更しない", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".zshrc": "new\n",
      });
      await writeTree(homeDir, {
        ".zshrc": "old\n",
      });

      const plan = await planLinkActions({
        dryRun: true,
        sourceRoot,
        homeDir,
        symlinkPaths: new Set([".zshrc"]),
        timestamp: "20260325T120000",
      });
      await runLinkPlan(plan);

      expect(await readFile(path.join(homeDir, ".zshrc"), "utf8")).toBe("old\n");
      await expect(access(path.join(homeDir, ".dotfiles-backups", "20260325T120000", ".zshrc"))).rejects.toThrow();
    });
  });

  test("後続の配備に失敗したら先に作成・置換した配備先を戻す", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".config/example/first.json": "first\n",
        ".config/example/second.json": "second\n",
        ".config/example/third.json": "third\n",
      });
      await writeTree(homeDir, {
        ".config/example/first.json": "old\n",
      });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        copyPaths: new Set([".config/example"]),
        symlinkPaths: new Set(),
      });
      const copyActions = plan.actions.filter((action) => action.type === "copy");
      const backupAction = plan.actions.find((action) => action.type === "backup");
      expect(copyActions).toHaveLength(3);
      expect(backupAction?.type).toBe("backup");
      await unlink(copyActions[2].sourcePath);

      await expect(runLinkPlan(plan)).rejects.toThrow();

      expect(await readFile(copyActions[0].destinationPath, "utf8")).toBe("old\n");
      for (const action of copyActions.slice(1)) {
        await expect(access(action.destinationPath)).rejects.toThrow();
      }
      if (backupAction?.type === "backup") {
        await expect(access(backupAction.backupPath)).rejects.toThrow();
      }
      await expect(access(plan.deploymentState.statePath)).rejects.toThrow();
    });
  });

  test("所有台帳の保存に失敗したら作成した配備先と退避対象を元に戻す", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      const stateDir = path.join(tempDir, "blocked-state");
      const statePath = path.join(stateDir, "deployment.json");
      const copiedPath = path.join(homeDir, ".config", "example.json");
      const prunedPath = path.join(homeDir, ".obsolete");
      await writeTree(sourceRoot, {
        ".config/example.json": "managed\n",
      });
      await writeTree(homeDir, {
        ".obsolete": "preserve\n",
      });

      const plan = await planLinkActions({
        copyPaths: new Set([".config"]),
        homeDir,
        prunePaths: new Set([".obsolete"]),
        sourceRoot,
        statePath,
        symlinkPaths: new Set(),
        timestamp: "20260802T120000",
      });
      await writeFile(stateDir, "not a directory\n");

      await expect(runLinkPlan(plan)).rejects.toThrow();

      await expect(access(copiedPath)).rejects.toThrow();
      expect(await readFile(prunedPath, "utf8")).toBe("preserve\n");
      await expect(access(path.join(
        homeDir,
        ".dotfiles-backups",
        "20260802T120000",
        ".obsolete",
      ))).rejects.toThrow();
    });
  });

  test("管理外の既存ファイルを残したままシンボリックリンクを張る", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".config/nvim/init.vim": "set number\n",
      });
      await writeTree(homeDir, {
        ".config/nvim/old.vim": "legacy\n",
      });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        symlinkPaths: new Set([".config/nvim"]),
      });
      await runLinkPlan(plan);

      expect(await readSymlinkTarget(path.join(homeDir, ".config", "nvim", "init.vim"))).toBe(await realpath(path.join(sourceRoot, ".config", "nvim", "init.vim")));
      expect(await readFile(path.join(homeDir, ".config", "nvim", "old.vim"), "utf8")).toBe("legacy\n");
    });
  });

  test("home/のファイルを動かさず、旧形式のディレクトリリンクを移行する", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      const sourceDir = path.join(sourceRoot, ".zsh.d");
      const sourceFile = path.join(sourceDir, "alias.zsh");
      const destinationDir = path.join(homeDir, ".zsh.d");
      const backupDir = path.join(homeDir, ".dotfiles-backups", "20260326T120000", ".zsh.d");
      await writeTree(sourceRoot, {
        ".zsh.d/alias.zsh": "alias ll='ls -la'\n",
      });
      await createSymlink(sourceDir, destinationDir);

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        symlinkPaths: new Set([".zsh.d"]),
        timestamp: "20260326T120000",
      });
      await runLinkPlan(plan);

      expect((await lstat(destinationDir)).isDirectory()).toBe(true);
      expect((await lstat(destinationDir)).isSymbolicLink()).toBe(false);
      expect(await readSymlinkTarget(path.join(destinationDir, "alias.zsh"))).toBe(await realpath(sourceFile));
      expect(await readFile(sourceFile, "utf8")).toBe("alias ll='ls -la'\n");
      expect((await lstat(backupDir)).isSymbolicLink()).toBe(true);
      expect(await readSymlinkTarget(backupDir)).toBe(await realpath(sourceDir));
    });
  });
});

describe("コピーによる配備", () => {
  test("シンボリックリンクではなく実ファイルを作る", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".claude/settings.json": '{"model":"opus"}',
      });
      await mkdir(homeDir, { recursive: true });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        copyPaths: new Set([".claude/settings.json"]),
        symlinkPaths: new Set(),
      });
      await runLinkPlan(plan);

      const copiedFile = path.join(homeDir, ".claude", "settings.json");
      const stat = await lstat(copiedFile);
      expect(stat.isFile()).toBe(true);
      expect(stat.isSymbolicLink()).toBe(false);
      expect(await readFile(copiedFile, "utf8")).toBe('{"model":"opus"}');
    });
  });

  test("既存ファイルをバックアップしてからコピーする", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".claude/settings.json": '{"model":"opus"}',
      });
      await writeTree(homeDir, {
        ".claude/settings.json": '{"model":"sonnet"}',
      });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        copyPaths: new Set([".claude/settings.json"]),
        symlinkPaths: new Set(),
        timestamp: "20260327T120000",
      });
      await runLinkPlan(plan);

      const copiedFile = path.join(homeDir, ".claude", "settings.json");
      expect(await readFile(copiedFile, "utf8")).toBe('{"model":"opus"}');

      const backupFile = path.join(homeDir, ".dotfiles-backups", "20260327T120000", ".claude", "settings.json");
      expect(await readFile(backupFile, "utf8")).toBe('{"model":"sonnet"}');
    });
  });

  test("置き換え用のコピーに失敗したら既存の配備先を残す", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      const sourceFile = path.join(sourceRoot, ".claude", "settings.json");
      const destinationFile = path.join(homeDir, ".claude", "settings.json");
      await writeTree(sourceRoot, {
        ".claude/settings.json": '{"model":"opus"}',
      });
      await writeTree(homeDir, {
        ".claude/settings.json": '{"model":"sonnet"}',
      });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        copyPaths: new Set([".claude/settings.json"]),
        symlinkPaths: new Set(),
        timestamp: "20260327T130000",
      });
      await rm(sourceFile);

      await expect(runLinkPlan(plan)).rejects.toThrow();
      expect(await readFile(destinationFile, "utf8")).toBe('{"model":"sonnet"}');
      await expect(access(path.join(homeDir, ".dotfiles-backups", "20260327T130000", ".claude", "settings.json"))).rejects.toThrow();
    });
  });

  test("copyにディレクトリを指定したら配下の全ファイルをコピーする", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".claude/skills/foo.md": "# foo\n",
        ".claude/skills/bar.md": "# bar\n",
      });
      await mkdir(homeDir, { recursive: true });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        copyPaths: new Set([".claude/skills"]),
        symlinkPaths: new Set(),
      });
      await runLinkPlan(plan);

      for (const name of ["foo.md", "bar.md"]) {
        const copiedFile = path.join(homeDir, ".claude", "skills", name);
        const stat = await lstat(copiedFile);
        expect(stat.isFile()).toBe(true);
        expect(stat.isSymbolicLink()).toBe(false);
        expect(await readFile(copiedFile, "utf8")).toBe(`# ${name.replace(".md", "")}\n`);
      }
    });
  });

  test("symlinkにもcopyにもないファイルは配備しない", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".zshrc": "listed\n",
        ".unlisted": "skip me\n",
      });
      await mkdir(homeDir, { recursive: true });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        symlinkPaths: new Set([".zshrc"]),
      });
      await runLinkPlan(plan);

      await expect(access(path.join(homeDir, ".zshrc"))).resolves.toBeNull();
      await expect(access(path.join(homeDir, ".unlisted"))).rejects.toThrow();
      expect(plan.actions.some((action) =>
        "sourcePath" in action && action.sourcePath.endsWith(".unlisted")
      )).toBe(false);
    });
  });
});

describe("不要になった配備先の退避", () => {
  test("初回実行でもhome/を指す孤児リンクを設定への列挙なしで検出する", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      const statePath = path.join(homeDir, ".local", "state", "dotfiles", "deployment.json");
      const obsoletePath = path.join(homeDir, ".tmux.conf");
      await writeTree(sourceRoot, {
        ".zshrc": "export TEST=1\n",
      });
      await createSymlink(path.join(sourceRoot, ".tmux.conf"), obsoletePath);

      const plan = await planLinkActions({
        dryRun: true,
        homeDir,
        sourceRoot,
        statePath,
        symlinkPaths: new Set([".zshrc"]),
        timestamp: "20260801T120000",
      });

      expect(plan.actions).toContainEqual({
        backupPath: path.join(homeDir, ".dotfiles-backups", "20260801T120000", ".tmux.conf"),
        destinationPath: obsoletePath,
        type: "prune",
      });
      expect(formatPlan(plan)).toContain("prune   ~/.tmux.conf");
      expect((await lstat(obsoletePath)).isSymbolicLink()).toBe(true);
      await expect(access(statePath)).rejects.toThrow();
    });
  });

  test("所有台帳にあるリンクがhome/から消えたら次のapplyで退避する", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      const statePath = path.join(homeDir, ".local", "state", "dotfiles", "deployment.json");
      const removedSource = path.join(sourceRoot, ".tmux.conf");
      const removedDestination = path.join(homeDir, ".tmux.conf");
      await writeTree(sourceRoot, {
        ".tmux.conf": "set -g mouse on\n",
        ".zshrc": "export TEST=1\n",
      });

      const firstPlan = await planLinkActions({
        homeDir,
        sourceRoot,
        statePath,
        symlinkPaths: new Set([".tmux.conf", ".zshrc"]),
      });
      await runLinkPlan(firstPlan);
      await unlink(removedSource);

      const secondPlan = await planLinkActions({
        homeDir,
        sourceRoot,
        statePath,
        symlinkPaths: new Set([".zshrc"]),
        timestamp: "20260801T121000",
      });
      await runLinkPlan(secondPlan);

      await expect(lstat(removedDestination)).rejects.toThrow();
      expect(
        (await lstat(path.join(homeDir, ".dotfiles-backups", "20260801T121000", ".tmux.conf"))).isSymbolicLink(),
      ).toBe(true);
      const state = JSON.parse(await readFile(statePath, "utf8"));
      expect(state.entries.map((entry: { path: string }) => entry.path)).toEqual([".zshrc"]);
    });
  });

  test("所有台帳の作成後は台帳にないリンクを推測で退避しない", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      const statePath = path.join(homeDir, ".local", "state", "dotfiles", "deployment.json");
      const unmanagedSource = path.join(sourceRoot, "unmanaged");
      const unmanagedDestination = path.join(homeDir, ".unmanaged");
      await writeTree(sourceRoot, {
        ".zshrc": "export TEST=1\n",
        unmanaged: "not configured\n",
      });

      await runLinkPlan(await planLinkActions({
        homeDir,
        sourceRoot,
        statePath,
        symlinkPaths: new Set([".zshrc"]),
      }));
      await createSymlink(unmanagedSource, unmanagedDestination);

      const plan = await planLinkActions({
        homeDir,
        sourceRoot,
        statePath,
        symlinkPaths: new Set([".zshrc"]),
      });

      expect(plan.actions.some((action) => action.destinationPath === unmanagedDestination)).toBe(false);
      expect(plan.drifts).toEqual([]);
      expect((await lstat(unmanagedDestination)).isSymbolicLink()).toBe(true);
    });
  });

  test("所有していたリンクが通常ファイルへ差し替えられていたら削除せずドリフトにする", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      const statePath = path.join(homeDir, ".local", "state", "dotfiles", "deployment.json");
      const removedSource = path.join(sourceRoot, ".tmux.conf");
      const removedDestination = path.join(homeDir, ".tmux.conf");
      await writeTree(sourceRoot, {
        ".tmux.conf": "set -g mouse on\n",
        ".zshrc": "export TEST=1\n",
      });

      await runLinkPlan(await planLinkActions({
        homeDir,
        sourceRoot,
        statePath,
        symlinkPaths: new Set([".tmux.conf", ".zshrc"]),
      }));
      await unlink(removedDestination);
      await writeFile(removedDestination, "locally owned\n");
      await unlink(removedSource);

      const plan = await planLinkActions({
        homeDir,
        sourceRoot,
        statePath,
        symlinkPaths: new Set([".zshrc"]),
      });
      await runLinkPlan(plan);

      expect(await readFile(removedDestination, "utf8")).toBe("locally owned\n");
      expect(plan.drifts).toContainEqual({
        destinationPath: removedDestination,
        reason: "previously managed symlink was replaced",
      });
      expect(formatPlan(plan)).toContain("drift   ~/.tmux.conf (previously managed symlink was replaced)");
      const state = JSON.parse(await readFile(statePath, "utf8"));
      expect(state.entries.map((entry: { path: string }) => entry.path)).toContain(".tmux.conf");
    });
  });

  test("所有台帳にある未変更のコピーがhome/から消えたら退避する", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      const statePath = path.join(homeDir, ".local", "state", "dotfiles", "deployment.json");
      const sourcePath = path.join(sourceRoot, ".claude", "settings.json");
      const destinationPath = path.join(homeDir, ".claude", "settings.json");
      await writeTree(sourceRoot, {
        ".claude/settings.json": "{}\n",
      });

      await runLinkPlan(await planLinkActions({
        copyPaths: new Set([".claude"]),
        homeDir,
        sourceRoot,
        statePath,
        symlinkPaths: new Set(),
      }));
      await unlink(sourcePath);

      const plan = await planLinkActions({
        copyPaths: new Set([".claude"]),
        homeDir,
        sourceRoot,
        statePath,
        symlinkPaths: new Set(),
        timestamp: "20260801T122000",
      });
      await runLinkPlan(plan);

      await expect(access(destinationPath)).rejects.toThrow();
      expect(await readFile(path.join(homeDir, ".dotfiles-backups", "20260801T122000", ".claude", "settings.json"), "utf8")).toBe("{}\n");
    });
  });

  test("所有していたコピーが編集されていたら削除せずドリフトにする", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      const statePath = path.join(homeDir, ".local", "state", "dotfiles", "deployment.json");
      const sourcePath = path.join(sourceRoot, ".claude", "settings.json");
      const destinationPath = path.join(homeDir, ".claude", "settings.json");
      await writeTree(sourceRoot, {
        ".claude/settings.json": "{}\n",
      });

      await runLinkPlan(await planLinkActions({
        copyPaths: new Set([".claude"]),
        homeDir,
        sourceRoot,
        statePath,
        symlinkPaths: new Set(),
      }));
      await writeFile(destinationPath, "{\"local\":true}\n");
      await unlink(sourcePath);

      const plan = await planLinkActions({
        copyPaths: new Set([".claude"]),
        homeDir,
        sourceRoot,
        statePath,
        symlinkPaths: new Set(),
      });
      await runLinkPlan(plan);

      expect(await readFile(destinationPath, "utf8")).toBe("{\"local\":true}\n");
      expect(plan.drifts).toContainEqual({
        destinationPath,
        reason: "previously managed copy was modified",
      });
    });
  });

  test("リポジトリ設定に残る不要なリンク切れのシンボリックリンクを退避する", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const repoRoot = process.cwd();
      const sourceRoot = path.join(repoRoot, "home");
      const homeDir = path.join(tempDir, "home");
      const obsoletePaths = [
        ".agents/AGENTS.md",
        ".config/zellij",
        "mybin/nyanpasu",
      ];
      for (const relativePath of obsoletePaths) {
        await createSymlink(
          path.join(tempDir, "old-home", relativePath),
          path.join(homeDir, relativePath),
        );
      }
      const { copyPaths, prunePaths, symlinkPaths } = await loadDotfilesConfig(
        repoRoot,
        sourceRoot,
      );

      const plan = await planLinkActions({
        copyPaths,
        homeDir,
        prunePaths,
        sourceRoot,
        symlinkPaths,
        timestamp: "20260328T100000",
      });
      await runLinkPlan(plan);

      for (const relativePath of obsoletePaths) {
        await expect(lstat(path.join(homeDir, relativePath))).rejects.toThrow();
        const backupPath = path.join(
          homeDir,
          ".dotfiles-backups",
          "20260328T100000",
          relativePath,
        );
        expect((await lstat(backupPath)).isSymbolicLink()).toBe(true);
      }
    });
  });

  test("home/にない最上位パスもpruneに指定されていれば退避する", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      await mkdir(sourceRoot, { recursive: true });
      await writeTree(homeDir, {
        ".Brewfile": "brew \"git\"\n",
      });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        prunePaths: new Set([".Brewfile"]),
        symlinkPaths: new Set(),
        timestamp: "20260328T110000",
      });
      await runLinkPlan(plan);

      await expect(access(path.join(homeDir, ".Brewfile"))).rejects.toThrow();
      expect(await readFile(path.join(homeDir, ".dotfiles-backups", "20260328T110000", ".Brewfile"), "utf8")).toBe('brew "git"\n');
    });
  });

  test("prune配下でhome/から消えたファイルを退避する", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".agents/skills/current/SKILL.md": "# current\n",
      });
      await writeTree(homeDir, {
        ".agents/skills/current/SKILL.md": "# current\n",
        ".agents/skills/removed/SKILL.md": "# removed\n",
      });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        copyPaths: new Set([".agents"]),
        prunePaths: new Set([".agents/skills"]),
        symlinkPaths: new Set(),
        timestamp: "20260328T120000",
      });
      await runLinkPlan(plan);

      await expect(access(path.join(homeDir, ".agents", "skills", "removed", "SKILL.md"))).rejects.toThrow();
      expect(await readFile(path.join(homeDir, ".dotfiles-backups", "20260328T120000", ".agents", "skills", "removed", "SKILL.md"), "utf8")).toBe("# removed\n");
      expect(await readFile(path.join(homeDir, ".agents", "skills", "current", "SKILL.md"), "utf8")).toBe("# current\n");
    });
  });

  test("pruneの対象外にある管理外のファイルは退避しない", async () => {
    await withTempDir("link-home", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "repo", "home");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".agents/skills/current/SKILL.md": "# current\n",
      });
      await writeTree(homeDir, {
        ".agents/local.txt": "keep\n",
      });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        copyPaths: new Set([".agents"]),
        prunePaths: new Set([".agents/skills"]),
        symlinkPaths: new Set(),
        timestamp: "20260328T120000",
      });
      await runLinkPlan(plan);

      expect(await readFile(path.join(homeDir, ".agents", "local.txt"), "utf8")).toBe("keep\n");
    });
  });
});

describe("配備計画の表示", () => {
  test("操作を一覧化し、件数を添える", () => {
    const plan: LinkPlan = {
      actions: [
        { type: "backup", sourcePath: "/repo/home/.zshrc", destinationPath: "/home/.zshrc", backupPath: "/home/.dotfiles-backups/20260418T150000/.zshrc" },
        { type: "link", sourcePath: "/repo/home/.zshrc", destinationPath: "/home/.zshrc" },
        { type: "copy", sourcePath: "/repo/home/.claude/settings.json", destinationPath: "/home/.claude/settings.json" },
        { type: "prune", destinationPath: "/home/.agents/skills/removed/SKILL.md", backupPath: "/home/.dotfiles-backups/20260418T150000/.agents/skills/removed/SKILL.md" },
        { type: "noop", sourcePath: "/repo/home/.gitconfig", destinationPath: "/home/.gitconfig" },
      ],
      backupRoot: "/home/.dotfiles-backups/20260418T150000",
      drifts: [],
      dryRun: true,
      homeDir: "/home",
      sourceRoot: "/repo/home",
      timestamp: "20260418T150000",
    };

    const output = formatPlan(plan);
    expect(output).toBe(
      [
        "  backup  ~/.zshrc → ~/.dotfiles-backups/20260418T150000/.zshrc",
        "  link    home/.zshrc → ~/.zshrc",
        "  copy    home/.claude/settings.json → ~/.claude/settings.json",
        "  prune   ~/.agents/skills/removed/SKILL.md → ~/.dotfiles-backups/20260418T150000/.agents/skills/removed/SKILL.md",
        "",
        "1 link, 1 copy, 1 prune, 1 backup, 1 unchanged",
      ].join("\n"),
    );
  });

  test("変更がなければ集計だけを表示する", () => {
    const plan: LinkPlan = {
      actions: [
        { type: "noop", sourcePath: "/repo/home/.zshrc", destinationPath: "/home/.zshrc" },
      ],
      backupRoot: "/home/.dotfiles-backups/20260418T150000",
      drifts: [],
      dryRun: true,
      homeDir: "/home",
      sourceRoot: "/repo/home",
      timestamp: "20260418T150000",
    };

    const output = formatPlan(plan);
    expect(output).toBe("1 unchanged");
  });
});
