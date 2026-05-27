#!/usr/bin/env bun

import path from "node:path";
import {
  finalizeCompiledAgents,
  readFileIfExists,
  restoreLockfileIfOnlyGeneratedAtChanged,
} from "./lib/agents-build";

async function main() {
  const cwd = process.cwd();
  const lockPath = path.join(cwd, "apm.lock.yaml");
  const originalLockfile = await readFileIfExists(lockPath);

  await run("apm", ["install", "--frozen", "--only", "apm", "--target", "claude,codex"], cwd);
  await run("apm", ["compile", "--clean", "--target", "claude,codex"], cwd);
  await finalizeCompiledAgents(cwd);
  await restoreLockfileIfOnlyGeneratedAtChanged(lockPath, originalLockfile);
}

async function run(command: string, args: string[], cwd: string) {
  const proc = Bun.spawn([command, ...args], {
    cwd,
    stderr: "inherit",
    stdout: "inherit",
  });
  const exitCode = await proc.exited;
  if (exitCode !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with exit code ${exitCode}`);
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
