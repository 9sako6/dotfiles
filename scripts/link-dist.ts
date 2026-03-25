#!/usr/bin/env bun

import path from "node:path";
import { planLinkActions, runLinkPlan, summarizePlan } from "./lib/link-dist";
import { parseCliArgs } from "./lib/paths";

async function main() {
  const { dryRun } = parseCliArgs(process.argv.slice(2));
  const homeDir = process.env.HOME;
  if (!homeDir) {
    throw new Error("HOME is not set");
  }

  const sourceRoot = path.resolve(process.cwd(), "dist");
  const plan = await planLinkActions({ dryRun, homeDir, sourceRoot });
  await runLinkPlan(plan);

  const summary = summarizePlan(plan);
  const mode = dryRun ? "dry-run" : "apply";
  console.log(
    JSON.stringify(
      {
        mode,
        ...summary,
      },
      null,
      2,
    ),
  );
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
