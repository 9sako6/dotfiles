#!/usr/bin/env bun

import path from "node:path";
import { loadDotfilesConfig } from "./lib/dotfiles-config";
import { formatPlan, planLinkActions, runLinkPlan } from "./lib/link-dist";
import { parseCliArgs } from "./lib/paths";

async function main() {
  const { dryRun } = parseCliArgs(process.argv.slice(2));
  const homeDir = process.env.HOME;
  if (!homeDir) {
    throw new Error("HOME is not set");
  }

  const repoRoot = process.cwd();
  const sourceRoot = path.resolve(repoRoot, "dist");
  const { copyPaths, symlinkPaths } = await loadDotfilesConfig(repoRoot, sourceRoot);
  const plan = await planLinkActions({
    copyPaths,
    dryRun,
    homeDir,
    sourceRoot,
    symlinkPaths,
  });
  await runLinkPlan(plan);

  console.log(formatPlan(plan));
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
