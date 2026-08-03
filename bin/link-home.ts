#!/usr/bin/env bun

import path from "node:path";
import { loadDotfilesConfig } from "../lib/dotfiles-config";
import { deploymentStatePath, withDeploymentLock } from "../lib/deployment-state";
import { discardLinkPlan, formatPlan, planLinkActions, runLinkPlan } from "../lib/link-home";

async function main() {
  const mode = parseMode(process.argv.slice(2));
  const dryRun = mode === "plan";
  const homeDir = process.env.HOME;
  if (!homeDir) {
    throw new Error("HOME is not set");
  }

  const repoRoot = process.cwd();
  const sourceRoot = path.resolve(repoRoot, "home");
  const { copyPaths, prunePaths, symlinkPaths } = await loadDotfilesConfig(repoRoot, sourceRoot);
  const statePath = deploymentStatePath(homeDir, process.env.XDG_STATE_HOME);
  const createPlan = () => planLinkActions({
    copyPaths,
    dryRun,
    homeDir,
    prunePaths,
    sourceRoot,
    statePath,
    symlinkPaths,
  });
  if (dryRun) {
    console.log(formatPlan(await createPlan()));
    return;
  }

  await withDeploymentLock(statePath, async () => {
    const plan = await createPlan();
    if (mode === "confirm") {
      console.log(formatPlan(plan));
      const actionCount = plan.actions.filter((action) => action.type !== "noop").length;
      if (actionCount === 0) {
        return;
      }
      if (!(await confirmApply())) {
        await discardLinkPlan(plan);
        console.log("Apply cancelled.");
        process.exitCode = 1;
        return;
      }
      await runLinkPlan(plan);
      console.log(`Applied ${actionCount} ${actionCount === 1 ? "change" : "changes"}.`);
      return;
    }
    await runLinkPlan(plan);
    console.log(formatPlan(plan));
  });
}

function parseMode(args: string[]): "apply" | "confirm" | "plan" {
  if (args.length === 0) return "apply";
  if (args.length === 1 && args[0] === "--check") return "plan";
  if (args.length === 1 && args[0] === "--confirm") return "confirm";
  throw new Error(`Unknown link-home arguments: ${args.join(" ")}`);
}

async function confirmApply(): Promise<boolean> {
  process.stdout.write("\nApply these changes? Type 'yes' to continue: ");
  return await new Promise((resolve) => {
    const reader = process.stdin.setEncoding("utf8");
    let input = "";
    const finish = (confirmed: boolean) => {
      reader.removeAllListeners("data");
      reader.removeAllListeners("end");
      process.stdout.write("\n");
      resolve(confirmed);
    };
    reader.on("data", (chunk: string) => {
      input += chunk;
      const newlineIndex = input.indexOf("\n");
      if (newlineIndex !== -1) {
        finish(input.slice(0, newlineIndex).replace(/\r$/, "") === "yes");
      }
    });
    reader.on("end", () => finish(input.replace(/\r$/, "") === "yes"));
  });
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
