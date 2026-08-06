#!/usr/bin/env bun

import { userInfo } from "node:os";
import path from "node:path";
import { resolveRepoRoot } from "../lib/paths";
import {
  parseSystemSourceRequest,
  resolveSystemSource,
  systemSourceDataRoot,
} from "../lib/system-source";

async function main(): Promise<void> {
  const [mode, ...args] = Bun.argv.slice(2);
  if (mode !== "plan" && mode !== "apply") {
    throw new Error("usage: system.ts <plan|apply> [--default|git-url]");
  }
  if (process.getuid?.() === 0) {
    throw new Error("run system tasks as the login user; sudo is requested when needed");
  }

  let gitUrl: string | undefined;
  let useDefault: string | undefined;
  for (const argument of args) {
    if (argument === "--default") {
      if (useDefault) throw new Error("--default may only be specified once");
      useDefault = "true";
    } else if (argument.startsWith("-")) {
      throw new Error(`unknown option: ${argument}`);
    } else if (gitUrl) {
      throw new Error("only one Git URL may be specified");
    } else {
      gitUrl = argument;
    }
  }

  const homeDir = Bun.env.HOME;
  if (!homeDir) throw new Error("HOME is not set");
  const repoRoot = await resolveRepoRoot(import.meta.path);
  const source = await resolveSystemSource(
    parseSystemSourceRequest(gitUrl, useDefault),
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
