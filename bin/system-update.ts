#!/usr/bin/env bun

import { resolveRepoRoot } from "../lib/paths";

async function run(command: string[], cwd: string): Promise<void> {
  console.log(`$ ${command.join(" ")}`);
  const process = Bun.spawn(command, {
    cwd,
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  });
  const exitCode = await process.exited;
  if (exitCode !== 0) {
    throw new Error(`${command[0]} exited with status ${exitCode}`);
  }
}

async function main(): Promise<void> {
  const repoRoot = await resolveRepoRoot(import.meta.path);

  console.log("==> update public nix-darwin and Home Manager inputs");
  await run(["nix", "flake", "update", "--flake", repoRoot], repoRoot);

  console.log("==> validate public system");
  await run(["dotfiles", "plan", "--default"], repoRoot);
  await run(["dotfiles", "test"], repoRoot);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
