#!/usr/bin/env bun

import { userInfo } from "node:os";
import path from "node:path";
import { resolveRepoRoot } from "../lib/paths";
import {
  parseSystemCommandArgs,
  resolveSystemSource,
  systemSourceDataRoot,
} from "../lib/system-source";

async function main(): Promise<void> {
  const { mode, request } = parseSystemCommandArgs(Bun.argv.slice(2));
  if (process.getuid?.() === 0) {
    throw new Error("run system tasks as the login user; sudo is requested when needed");
  }

  const homeDir = Bun.env.HOME;
  if (!homeDir) throw new Error("HOME is not set");
  const repoRoot = await resolveRepoRoot(import.meta.path);
  const source = await resolveSystemSource(
    request,
    {
      dataRoot: systemSourceDataRoot(homeDir, Bun.env.XDG_DATA_HOME),
      publicDirectory: path.join(repoRoot, "darwin"),
      selectionPath: "/etc/nix-darwin/flake.nix",
    },
  );

  console.log(source.kind === "default"
    ? "system source: public dotfiles (local checkout)"
    : `system source: ${source.url}`);
  console.log(`source revision: ${source.revision}`);
  const backend = path.join(repoRoot, "bin", "system-backend.sh");
  const processResult = Bun.spawn([
    backend,
    mode,
    source.kind,
    userInfo().username,
    source.directory,
    source.previousTarget ?? "missing",
    path.join(source.directory, "flake.nix"),
  ], {
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  });
  const exitCode = await processResult.exited;
  if (exitCode !== 0) process.exitCode = exitCode;
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
