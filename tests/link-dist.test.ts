import { describe, expect, test } from "bun:test";
import { access, mkdir, readFile, realpath } from "node:fs/promises";
import path from "node:path";
import { createSymlink, ensureDir, readSymlinkTarget, withTempDir, writeTree } from "./test-helpers";
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

  test("backs up a conflicting directory before relinking", async () => {
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

      expect(plan.actions).toHaveLength(2);
      expect(plan.actions[0]).toMatchObject({
        destinationPath: path.join(homeDir, ".config", "nvim"),
        type: "backup",
      });
      expect(plan.actions[1]).toMatchObject({
        destinationPath: path.join(homeDir, ".config", "nvim"),
        sourcePath: path.join(sourceRoot, ".config", "nvim"),
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

  test("links top-level managed directories as a whole", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        "mybin/tool": "#!/bin/sh\n",
        "mybin/lib/helper.rb": "puts :ok\n",
      });

      const plan = await planLinkActions({ sourceRoot, homeDir });

      expect(plan.actions).toHaveLength(1);
      expect(plan.actions[0]).toMatchObject({
        destinationPath: path.join(homeDir, "mybin"),
        sourcePath: path.join(sourceRoot, "mybin"),
        type: "link",
      });
    });
  });
});

describe("runLinkPlan", () => {
  test("creates parent directories and symlinks managed leaf directories", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".config/mise/config.toml": "tasks = {}\n",
      });
      await mkdir(homeDir, { recursive: true });

      const plan = await planLinkActions({ sourceRoot, homeDir });
      await runLinkPlan(plan);

      const linkedDir = path.join(homeDir, ".config", "mise");
      const linkedPath = path.join(linkedDir, "config.toml");
      await expect(access(linkedPath)).resolves.toBeNull();
      expect(await readSymlinkTarget(linkedDir)).toBe(await realpath(path.join(sourceRoot, ".config", "mise")));
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

  test("backs up directories before linking them", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        ".config/nvim/init.vim": "set number\n",
      });
      await writeTree(homeDir, {
        ".config/nvim/old.vim": "legacy\n",
      });
      await ensureDir(homeDir, ".config");

      const plan = await planLinkActions({ sourceRoot, homeDir, timestamp: "20260325T120000" });
      await runLinkPlan(plan);

      expect(await readFile(path.join(homeDir, ".dotfiles-backups", "20260325T120000", ".config", "nvim", "old.vim"), "utf8")).toBe("legacy\n");
      expect(await readSymlinkTarget(path.join(homeDir, ".config", "nvim"))).toBe(await realpath(path.join(sourceRoot, ".config", "nvim")));
    });
  });

  test("replaces a conflicting top-level directory with a symlink", async () => {
    await withTempDir("link-dist", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      const homeDir = path.join(tempDir, "home");
      await writeTree(sourceRoot, {
        "mybin/tool": "#!/bin/sh\n",
        "mybin/lib/helper.rb": "puts :ok\n",
      });
      await writeTree(homeDir, {
        "mybin/legacy.txt": "legacy\n",
      });

      const plan = await planLinkActions({ sourceRoot, homeDir, timestamp: "20260325T120000" });
      await runLinkPlan(plan);

      expect(await readFile(path.join(homeDir, ".dotfiles-backups", "20260325T120000", "mybin", "legacy.txt"), "utf8")).toBe("legacy\n");
      expect(await readSymlinkTarget(path.join(homeDir, "mybin"))).toBe(await realpath(path.join(sourceRoot, "mybin")));
    });
  });
});
