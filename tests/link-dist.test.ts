import { describe, expect, test } from "bun:test";
import { access, lstat, mkdir, readFile, realpath } from "node:fs/promises";
import path from "node:path";
import { createSymlink, readSymlinkTarget, withTempDir, writeTree } from "./test-helpers";
import { planLinkActions, runLinkPlan } from "../scripts/lib/link-dist";

describe("planLinkActions", () => {
  test("plans a symlink for a new file", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".zshrc": "export TEST=1\n",
      });
      await mkdir(homeDir, { recursive: true });

      const plan = await planLinkActions({ sourceRoot, homeDir });

      expect(plan.actions).toHaveLength(1);
      expect(plan.actions[0]).toMatchObject({
        destinationPath: path.join(homeDir, ".zshrc"),
        sourcePath: path.join(sourceRoot, ".zshrc"),
        type: "link",
      });
    });
  });

  test("keeps a matching symlink as no-op", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      const sourcePath = path.join(sourceRoot, ".zshrc");
      const destinationPath = path.join(homeDir, ".zshrc");
      await writeTree(sourceRoot, {
        ".zshrc": "export TEST=1\n",
      });
      await createSymlink(sourcePath, destinationPath);

      const plan = await planLinkActions({ sourceRoot, homeDir });

      expect(plan.actions).toHaveLength(1);
      expect(plan.actions[0]).toMatchObject({
        destinationPath,
        sourcePath,
        type: "noop",
      });
    });
  });

  test("backs up a conflicting file before relinking", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".zshrc": "export TEST=1\n",
      });
      await writeTree(homeDir, {
        ".zshrc": "legacy\n",
      });

      const plan = await planLinkActions({ sourceRoot, homeDir });

      expect(plan.actions).toHaveLength(2);
      expect(plan.actions[0]?.type).toBe("backup");
      expect(plan.actions[1]?.type).toBe("link");
      expect(plan.actions[0]?.backupPath).toContain(".dotfiles-backups");
    });
  });

  test("links files individually within nested directories", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".config/nvim/init.vim": "set number\n",
      });
      await mkdir(homeDir, { recursive: true });

      const plan = await planLinkActions({ sourceRoot, homeDir });

      expect(plan.actions).toHaveLength(1);
      expect(plan.actions[0]).toMatchObject({
        destinationPath: path.join(homeDir, ".config", "nvim", "init.vim"),
        sourcePath: path.join(sourceRoot, ".config", "nvim", "init.vim"),
        type: "link",
      });
    });
  });

  test("relinks a mismatched symlink", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      const elsewhere = path.join(tempDir, "elsewhere");
      await writeTree(sourceRoot, {
        ".gitconfig": "[user]\n",
      });
      await writeTree(elsewhere, {
        ".gitconfig": "[legacy]\n",
      });
      await createSymlink(path.join(elsewhere, ".gitconfig"), path.join(homeDir, ".gitconfig"));

      const plan = await planLinkActions({ sourceRoot, homeDir });

      expect(plan.actions).toHaveLength(2);
      expect(plan.actions[0]?.type).toBe("backup");
      expect(plan.actions[1]?.type).toBe("link");
    });
  });

  test("links files individually within directories with subdirectories", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        "mybin/tool": "#!/bin/sh\n",
        "mybin/lib/helper.rb": "puts :ok\n",
      });
      await mkdir(homeDir, { recursive: true });

      const plan = await planLinkActions({ sourceRoot, homeDir });

      expect(plan.actions).toHaveLength(2);
      const paths = plan.actions.map((a) => a.destinationPath).sort();
      expect(paths).toEqual([
        path.join(homeDir, "mybin", "lib", "helper.rb"),
        path.join(homeDir, "mybin", "tool"),
      ]);
      expect(plan.actions.every((a) => a.type === "link")).toBe(true);
    });
  });

  test("backs up an old managed directory symlink once before relinking child files", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      const sourceDir = path.join(sourceRoot, ".zsh.d");
      await writeTree(sourceRoot, {
        ".zsh.d/alias.zsh": "alias ll='ls -la'\n",
      });
      await createSymlink(sourceDir, path.join(homeDir, ".zsh.d"));

      const plan = await planLinkActions({ sourceRoot, homeDir, timestamp: "20260326T120000" });

      expect(plan.actions).toHaveLength(2);
      expect(plan.actions[0]).toMatchObject({
        backupPath: path.join(homeDir, ".dotfiles-backups", "20260326T120000", ".zsh.d"),
        destinationPath: path.join(homeDir, ".zsh.d"),
        sourcePath: sourceDir,
        type: "backup",
      });
      expect(plan.actions[1]).toMatchObject({
        destinationPath: path.join(homeDir, ".zsh.d", "alias.zsh"),
        sourcePath: path.join(sourceDir, "alias.zsh"),
        type: "link",
      });
    });
  });
});

describe("runLinkPlan", () => {
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

  test("creates codex config directories and symlinks AGENTS.md", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".codex/AGENTS.md": "# general\n\n- 日本語で話す\n",
      });
      await mkdir(homeDir, { recursive: true });

      const plan = await planLinkActions({ sourceRoot, homeDir });
      await runLinkPlan(plan);

      const linkedFile = path.join(homeDir, ".codex", "AGENTS.md");
      await expect(access(linkedFile)).resolves.toBeNull();
      expect(await readSymlinkTarget(linkedFile)).toBe(await realpath(path.join(sourceRoot, ".codex", "AGENTS.md")));
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

  test("links individual files within directories that have subdirectories", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        "mybin/tool": "#!/bin/sh\n",
        "mybin/lib/helper.rb": "puts :ok\n",
      });
      await mkdir(homeDir, { recursive: true });

      const plan = await planLinkActions({ sourceRoot, homeDir });
      await runLinkPlan(plan);

      expect(await readSymlinkTarget(path.join(homeDir, "mybin", "tool"))).toBe(await realpath(path.join(sourceRoot, "mybin", "tool")));
      expect(await readSymlinkTarget(path.join(homeDir, "mybin", "lib", "helper.rb"))).toBe(await realpath(path.join(sourceRoot, "mybin", "lib", "helper.rb")));
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
