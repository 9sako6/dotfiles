import { describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { inspectSelectedSystemSource } from "../lib/system-source";

describe("public system source migration", () => {
  test("旧darwin/flake.nix selectionをroot flakeのpublic sourceとして扱う", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "system-source-migration-"));
    try {
      const publicDirectory = path.join(root, "dotfiles");
      const selectionPath = path.join(root, "etc", "flake.nix");
      const legacyFlake = path.join(publicDirectory, "darwin", "flake.nix");
      await mkdir(publicDirectory, { recursive: true });
      await writeFile(path.join(publicDirectory, "flake.nix"), "{}\n");
      await mkdir(path.dirname(selectionPath), { recursive: true });
      await symlink(legacyFlake, selectionPath);

      const observed = await inspectSelectedSystemSource(
        {
          dataRoot: path.join(root, "data"),
          publicDirectory,
          selectionPath,
        },
        async (args) => args.includes("rev-parse") ? "0123456789abcdef" : "",
      );

      expect(observed.kind).toBe("default");
      expect(observed.directory).toBe(publicDirectory);
      expect(observed.previousTarget).toBe(legacyFlake);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
