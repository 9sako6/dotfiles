import { expect, test } from "bun:test";
import { access } from "node:fs/promises";
import { constants } from "node:fs";
import path from "node:path";

const repoRoot = process.cwd();

const criticalDistPaths = [
  "dist/.codex/AGENTS.md",
  "dist/.config/git/hooks/pre-commit",
  "dist/.config/mise/config.toml",
  "dist/.gitconfig",
  "dist/.zshenv",
  "dist/.zshrc",
  "dist/.zsh.d/alias.zsh",
  "dist/mybin/git-undo",
  "dist/mybin/tada",
] as const;

test("dist contains the critical managed entrypoints", async () => {
  await Promise.all(
    criticalDistPaths.map(async (relativePath) => {
      await expect(access(path.join(repoRoot, relativePath))).resolves.toBeNull();
    }),
  );
});

test("legacy managed home source tree is gone", async () => {
  await expect(access(path.join(repoRoot, "home"))).rejects.toThrow();
});

test("git and tada helper binaries stay executable", async () => {
  await expect(access(path.join(repoRoot, "dist/mybin/git-undo"), constants.X_OK)).resolves.toBeNull();
  await expect(access(path.join(repoRoot, "dist/mybin/tada"), constants.X_OK)).resolves.toBeNull();
  await expect(access(path.join(repoRoot, "dist/mybin/lib/tada-darwin-arm64"), constants.X_OK)).resolves.toBeNull();
});
