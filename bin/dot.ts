#!/usr/bin/env bun

import path from "node:path";
import {
  DotUsageError,
  formatDotHelp,
  hasUncommittedChanges,
  parseDotCommand,
  pullDotfiles,
  type DotCommand,
} from "../lib/dot";
import { resolveRepoRoot } from "../lib/paths";

async function main(): Promise<number> {
  let parsed;
  try {
    parsed = parseDotCommand(process.argv.slice(2));
  } catch (error) {
    if (error instanceof DotUsageError) {
      console.error(`dot: ${error.message}`);
      console.error("Run 'dot help' for usage.");
      return 2;
    }
    throw error;
  }

  if (parsed.type === "help") {
    console.log(formatDotHelp(parsed.command));
    return 0;
  }

  const repoRoot = await resolveRepoRoot(import.meta.path);
  try {
    if (parsed.command === "pull") {
      console.log(await pullDotfiles(repoRoot));
      return 0;
    }
    if (parsed.command === "plan" || parsed.command === "apply") {
      if (await hasUncommittedChanges(repoRoot)) {
        console.error("Warning: dotfiles source has uncommitted changes.");
      }
    }
    return await runCommand(repoRoot, parsed.command);
  } catch (error) {
    console.error(`dot: ${error instanceof Error ? error.message : error}`);
    return 1;
  }
}

async function runCommand(repoRoot: string, command: Exclude<DotCommand, "pull">): Promise<number> {
  const invocation = command === "doctor"
    ? ["doctor.ts"]
    : ["link-home.ts", command === "plan" ? "--check" : "--confirm"];
  const child = Bun.spawn([process.execPath, path.join(repoRoot, "bin", invocation[0]!), ...invocation.slice(1)], {
    env: Bun.env,
    stderr: "inherit",
    stdin: "inherit",
    stdout: "inherit",
  });
  return await child.exited;
}

main().then((exitCode) => {
  process.exitCode = exitCode;
}).catch((error) => {
  console.error(`dot: ${error instanceof Error ? error.message : error}`);
  process.exitCode = 1;
});
