import { describe, expect, test } from "bun:test";
import { access, lstat, mkdir, readFile, realpath } from "node:fs/promises";
import path from "node:path";
import { createSymlink, readSymlinkTarget, withTempDir, writeTree } from "./test-helpers";
import { planLinkActions, runLinkPlan } from "../scripts/lib/link-dist";

describe("runLinkPlan", () => {
  test("keeps a matching symlink unchanged", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      const sourcePath = path.join(sourceRoot, ".zshrc");
      const destinationPath = path.join(homeDir, ".zshrc");
      await writeTree(sourceRoot, {
        ".zshrc": "export TEST=1\n",
      });
      await createSymlink(sourcePath, destinationPath);

      const plan = await planLinkActions({ sourceRoot, homeDir, timestamp: "20260325T120000" });
      await runLinkPlan(plan);

      expect(await readSymlinkTarget(destinationPath)).toBe(await realpath(sourcePath));
      await expect(access(path.join(homeDir, ".dotfiles-backups", "20260325T120000", ".zshrc"))).rejects.toThrow();
    });
  });

  test("creates parent directories and symlinks individual files", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".config/mise/config.toml": "tasks = {}\n",
      });
      await mkdir(homeDir, { recursive: true });

      const plan = await planLinkActions({ sourceRoot, homeDir });
      await runLinkPlan(plan);

      const linkedFile = path.join(homeDir, ".config", "mise", "config.toml");
      await expect(access(linkedFile)).resolves.toBeNull();
      expect(await readSymlinkTarget(linkedFile)).toBe(await realpath(path.join(sourceRoot, ".config", "mise", "config.toml")));
    });
  });

  test("moves conflicting files into the backup tree", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".zshrc": "new\n",
      });
      await writeTree(homeDir, {
        ".zshrc": "old\n",
      });

      const plan = await planLinkActions({ sourceRoot, homeDir, timestamp: "20260325T120000" });
      await runLinkPlan(plan);

      const backupPath = path.join(homeDir, ".dotfiles-backups", "20260325T120000", ".zshrc");
      expect(await readFile(backupPath, "utf8")).toBe("old\n");
      expect(await readSymlinkTarget(path.join(homeDir, ".zshrc"))).toBe(await realpath(path.join(sourceRoot, ".zshrc")));
    });
  });

  test("does not mutate the filesystem in dry-run mode", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".zshrc": "new\n",
      });
      await writeTree(homeDir, {
        ".zshrc": "old\n",
      });

      const plan = await planLinkActions({ dryRun: true, sourceRoot, homeDir, timestamp: "20260325T120000" });
      await runLinkPlan(plan);

      expect(await readFile(path.join(homeDir, ".zshrc"), "utf8")).toBe("old\n");
      await expect(access(path.join(homeDir, ".dotfiles-backups", "20260325T120000", ".zshrc"))).rejects.toThrow();
    });
  });

  test("creates symlinks alongside existing unmanaged files", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".config/nvim/init.vim": "set number\n",
      });
      await writeTree(homeDir, {
        ".config/nvim/old.vim": "legacy\n",
      });

      const plan = await planLinkActions({ sourceRoot, homeDir });
      await runLinkPlan(plan);

      expect(await readSymlinkTarget(path.join(homeDir, ".config", "nvim", "init.vim"))).toBe(await realpath(path.join(sourceRoot, ".config", "nvim", "init.vim")));
      expect(await readFile(path.join(homeDir, ".config", "nvim", "old.vim"), "utf8")).toBe("legacy\n");
    });
  });

  test("migrates an old managed directory symlink without moving source files", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      const sourceDir = path.join(sourceRoot, ".zsh.d");
      const sourceFile = path.join(sourceDir, "alias.zsh");
      const destinationDir = path.join(homeDir, ".zsh.d");
      const backupDir = path.join(homeDir, ".dotfiles-backups", "20260326T120000", ".zsh.d");
      await writeTree(sourceRoot, {
        ".zsh.d/alias.zsh": "alias ll='ls -la'\n",
      });
      await createSymlink(sourceDir, destinationDir);

      const plan = await planLinkActions({ sourceRoot, homeDir, timestamp: "20260326T120000" });
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

describe("runLinkPlan with copy actions", () => {
  test("creates a real file (not symlink) for copy actions", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".claude/settings.json": '{"model":"opus"}',
      });
      await mkdir(homeDir, { recursive: true });

      const plan = await planLinkActions({
        sourceRoot,
        homeDir,
        copyPaths: new Set([".claude/settings.json"]),
      });
      await runLinkPlan(plan);

      const copiedFile = path.join(homeDir, ".claude", "settings.json");
      const stat = await lstat(copiedFile);
      expect(stat.isFile()).toBe(true);
      expect(stat.isSymbolicLink()).toBe(false);
      expect(await readFile(copiedFile, "utf8")).toBe('{"model":"opus"}');
    });
  });

  test("backs up existing file before copying", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
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
        timestamp: "20260327T120000",
      });
      await runLinkPlan(plan);

      const copiedFile = path.join(homeDir, ".claude", "settings.json");
      expect(await readFile(copiedFile, "utf8")).toBe('{"model":"opus"}');

      const backupFile = path.join(homeDir, ".dotfiles-backups", "20260327T120000", ".claude", "settings.json");
      expect(await readFile(backupFile, "utf8")).toBe('{"model":"sonnet"}');
    });
  });
});
