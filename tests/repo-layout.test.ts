import { expect, test } from "bun:test";
import { access } from "node:fs/promises";
import path from "node:path";

const repoRoot = process.cwd();

const expectedDistPaths = [
  "dist/.Brewfile",
  "dist/.config/mise/config.toml",
  "dist/.gitconfig",
  "dist/.gitignore_global",
  "dist/.zshenv",
  "dist/.zshrc",
  "dist/.zsh.d/alias.zsh",
  "dist/.zsh.d/functions.zsh",
  "dist/.zsh.d/keybindings.zsh",
  "dist/.zsh.d/prompt.zsh",
  "dist/alias.sh",
  "dist/mybin/diffcop",
  "dist/mybin/gomi",
  "dist/mybin/gppr",
  "dist/mybin/lein",
  "dist/mybin/lib/work_timer.rb",
  "dist/mybin/nonnonbiyori",
  "dist/mybin/nonnonbiyori.ascii",
  "dist/mybin/renchon",
  "dist/mybin/renchon.ascii",
  "dist/mybin/timer",
] as const;

test("dist contains the managed dotfiles layout", async () => {
  await Promise.all(
    expectedDistPaths.map(async (relativePath) => {
      await expect(access(path.join(repoRoot, relativePath))).resolves.toBeNull();
    }),
  );
});

test("legacy managed home source tree is gone", async () => {
  await expect(access(path.join(repoRoot, "home"))).rejects.toThrow();
});
