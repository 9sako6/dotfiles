import { describe, expect, test } from "bun:test";
import path from "node:path";
import { writeFile } from "node:fs/promises";
import { withTempDir, writeTree } from "./test-helpers";
import { loadDotfilesConfig } from "../lib/dotfiles-config";

describe(".dotfiles.jsonの読み込み", () => {
  test("symlink、copy、pruneの各設定を読み込む", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const repoRoot = tempDir;
      const sourceRoot = path.join(tempDir, "home");
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

  test("symlink、copy、pruneがなければ空集合として扱う", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "home");
      await writeTree(sourceRoot, { ".keep": "" });
      await writeFile(path.join(tempDir, ".dotfiles.json"), "{}");

      const config = await loadDotfilesConfig(tempDir, sourceRoot);
      expect(config.symlinkPaths.size).toBe(0);
      expect(config.copyPaths.size).toBe(0);
      expect(config.prunePaths.size).toBe(0);
    });
  });

  test(".dotfiles.jsonがなければエラーにする", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "home");
      await writeTree(sourceRoot, { ".keep": "" });
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow();
    });
  });

  test(".dotfiles.jsonが正しいJSONでなければエラーにする", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "home");
      await writeTree(sourceRoot, { ".keep": "" });
      await writeFile(path.join(tempDir, ".dotfiles.json"), "{not json");
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow();
    });
  });

  test("symlinkが文字列の配列でなければエラーにする", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "home");
      await writeTree(sourceRoot, { ".keep": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ symlink: [1, 2] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/symlink/);
    });
  });

  test("管理対象のパスがhome配下から外れていればエラーにする", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "home");
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

  test("pruneが文字列の配列でなければエラーにする", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "home");
      await writeTree(sourceRoot, { ".keep": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ prune: [1, 2] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/prune/);
    });
  });

  test("同じパスがsymlinkとcopyの両方にあればエラーにする", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "home");
      await writeTree(sourceRoot, { ".zshrc": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ symlink: [".zshrc"], copy: [".zshrc"] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/\.zshrc/);
    });
  });

  test("copyのパスがsymlink配下にあればエラーにする", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "home");
      await writeTree(sourceRoot, { ".config/mise/config.toml": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ symlink: [".config"], copy: [".config/mise"] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/\.config/);
    });
  });

  test("symlinkのパスがcopy配下にあればエラーにする", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "home");
      await writeTree(sourceRoot, { ".claude/settings.json": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ symlink: [".claude/settings.json"], copy: [".claude"] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/\.claude/);
    });
  });

  test("pruneのパスがcopy配下になければエラーにする", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "home");
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

  test("列挙したパスがhome/になければエラーにする", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "home");
      await writeTree(sourceRoot, { ".zshrc": "" });
      await writeFile(
        path.join(tempDir, ".dotfiles.json"),
        JSON.stringify({ symlink: [".zshrc", ".not-there"] }),
      );
      await expect(loadDotfilesConfig(tempDir, sourceRoot)).rejects.toThrow(/\.not-there/);
    });
  });

  test("home/に存在するディレクトリを受け入れる", async () => {
    await withTempDir("dotfiles-config", async (tempDir) => {
      const sourceRoot = path.join(tempDir, "home");
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
