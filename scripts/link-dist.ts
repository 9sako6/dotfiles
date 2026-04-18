#!/usr/bin/env bun

import { readFile } from "node:fs/promises";
import path from "node:path";
import { planLinkActions, runLinkPlan, summarizePlan } from "./lib/link-dist";
import { parseCliArgs } from "./lib/paths";

async function loadCopyPaths(repoRoot: string): Promise<Set<string>> {
  const configPath = path.join(repoRoot, ".dotfiles.json");
  try {
    const raw = await readFile(configPath, "utf8");
    const config = JSON.parse(raw) as { copy?: unknown };
    if (Array.isArray(config.copy)) {
      return new Set(config.copy.filter((x): x is string => typeof x === "string"));
    }
  } catch {
    // .dotfiles.json が存在しない、または parse エラーの場合は空 Set
  }
  return new Set();
}

async function main() {
  const { dryRun } = parseCliArgs(process.argv.slice(2));
  const homeDir = process.env.HOME;
  if (!homeDir) {
    throw new Error("HOME is not set");
  }

  const repoRoot = process.cwd();
  const sourceRoot = path.resolve(repoRoot, "dist");
  const copyPaths = await loadCopyPaths(repoRoot);
  const plan = await planLinkActions({ dryRun, homeDir, sourceRoot, copyPaths });
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
