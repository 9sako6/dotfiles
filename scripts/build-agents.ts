#!/usr/bin/env bun

import path from "node:path";
import {
  type AgentsBuildOperation,
  createAgentsBuildPlan,
  finalizeCompiledAgents,
  readFileIfExists,
  restoreLockfileIfOnlyGeneratedAtChanged,
} from "./lib/agents-build";

async function main() {
  const cwd = process.cwd();
  const [operationArg = "build", ...operationArgs] = process.argv.slice(2);
  if (!isAgentsBuildOperation(operationArg)) {
    throw new Error(`Unknown agents operation: ${operationArg}`);
  }

  const lockPath = path.join(cwd, "apm.lock.yaml");
  const originalLockfile = await readFileIfExists(lockPath);

  for (const commandPlan of createAgentsBuildPlan(operationArg, operationArgs)) {
    await run(commandPlan.command, commandPlan.args, cwd);
  }
  await finalizeCompiledAgents(cwd);
  await restoreLockfileIfOnlyGeneratedAtChanged(lockPath, originalLockfile);
}

function isAgentsBuildOperation(value: string): value is AgentsBuildOperation {
  return ["build", "install", "update", "uninstall"].includes(value);
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
