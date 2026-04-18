import { describe, expect, test } from "bun:test";
import path from "node:path";
import { writeFile } from "node:fs/promises";
import { withTempDir, writeTree } from "./test-helpers";
import { loadDotfilesConfig } from "../scripts/lib/dotfiles-config";

describe("loadDotfilesConfig", () => {
  test("returns symlinkPaths and copyPaths from .dotfiles.json", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const repoRoot = tempDir;
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, {
        ".zshrc": "",
        ".claude/settings.json": "{}",
      });
      await writeFile(
        path.join(repoRoot, ".dotfiles.json"),
        JSON.stringify({ symlink: [".zshrc"], copy: [".claude"] }),
      );

      const config = await loadDotfilesConfig(repoRoot, sourceRoot);

      expect(config.symlinkPaths).toEqual(new Set([".zshrc"]));
      expect(config.copyPaths).toEqual(new Set([".claude"]));
    });
  });

  test("treats missing symlink and copy fields as empty sets", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".keep": "" });
      await writeFile(path.join(tempDir, ".dotfiles.json"), "{}");

      const config = await loadDotfilesConfig(tempDir, sourceRoot);
      expect(config.symlinkPaths.size).toBe(0);
      expect(config.copyPaths.size).toBe(0);
    });
  });

  test("throws when .dotfiles.json is missing", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".keep": "" });
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow();
    });
  });

  test("throws when .dotfiles.json is not valid JSON", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".keep": "" });
      await writeFile(path.join(tempDir, ".dotfiles.json"), "{not json");
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow();
    });
  });

  test("throws when symlink is not a string array", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".keep": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ symlink: [1, 2] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/symlink/);
    });
  });

  test("throws when the same path appears in both symlink and copy", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".zshrc": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ symlink: [".zshrc"], copy: [".zshrc"] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/\.zshrc/);
    });
  });

  test("throws when a copy path is a descendant of a symlink path", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".config/mise/config.toml": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ symlink: [".config"], copy: [".config/mise"] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/\.config/);
    });
  });

  test("throws when a symlink path is a descendant of a copy path", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".claude/settings.json": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ symlink: [".claude/settings.json"], copy: [".claude"] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/\.claude/);
    });
  });

  test("throws when a listed path does not exist under sourceRoot", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".zshrc": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ symlink: [".zshrc", ".not-there"] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/\.not-there/);
    });
  });

  test("accepts directory entries that exist under sourceRoot", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".zsh.d/alias.zsh": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ symlink: [".zsh.d"] }),
      );

      const config = await loadDotfilesConfig(tempDir, sourceRoot);
      expect(config.symlinkPaths).toEqual(new Set([".zsh.d"]));
    });
  });
});
