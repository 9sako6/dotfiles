import { describe, expect, test } from "bun:test";
import path from "node:path";
import { writeFile } from "node:fs/promises";
import { withTempDir, writeTree } from "./test-helpers";
import { loadDotfilesConfig } from "../scripts/lib/dotfiles-config";

describe("loadDotfilesConfig", () => {
  test("returns symlinkPaths, copyPaths, and prunePaths from .dotfiles.json", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const repoRoot = tempDir;
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, {
        ".zshrc": "",
        ".claude/settings.json": "{}",
        ".agents/skills/foo/SKILL.md": "# foo\n",
      });
      await writeFile(
        path.join(repoRoot, ".dotfiles.json"),
        JSON.stringify({ symlink: [".zshrc"], copy: [".agents", ".claude"], prune: [".agents/skills"] }),
      );

      const config = await loadDotfilesConfig(repoRoot, sourceRoot);

      expect(config.symlinkPaths).toEqual(new Set([".zshrc"]));
      expect(config.copyPaths).toEqual(new Set([".agents", ".claude"]));
      expect(config.prunePaths).toEqual(new Set([".agents/skills"]));
    });
  });

  test("treats missing symlink, copy, and prune fields as empty sets", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".keep": "" });
      await writeFile(path.join(tempDir, ".dotfiles.json"), "{}");

      const config = await loadDotfilesConfig(tempDir, sourceRoot);
      expect(config.symlinkPaths.size).toBe(0);
      expect(config.copyPaths.size).toBe(0);
      expect(config.prunePaths.size).toBe(0);
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

  test("throws when a managed path escapes the dist tree", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".keep": "" });

      for (const config of [
        { symlink: ["../outside"] },
        { copy: ["/absolute"] },
        { prune: [""] },
      ]) {
        await writeFile(path.join(tempDir, ".dotfiles.json"), JSON.stringify(config));
        await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/path/);
      }
    });
  });

  test("throws when prune is not a string array", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".keep": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ prune: [1, 2] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/prune/);
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

  test("throws when a prune path is outside copy paths", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, {
        ".agents/skills/foo/SKILL.md": "",
      });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ copy: [".claude"], prune: [".agents/skills"] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/prune/);
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

  test("throws when a prune path does not exist under sourceRoot", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "dist");
      await writeTree(sourceRoot, { ".zshrc": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ prune: [".agents/skills"] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/\.agents\/skills/);
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
