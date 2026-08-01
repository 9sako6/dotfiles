#!/usr/bin/env bun

import { access } from "node:fs/promises";
import path from "node:path";
import { loadDotfilesConfig } from "./lib/dotfiles-config";
import { deploymentStatePath } from "./lib/deployment-state";
import { formatDoctorSection, inspectHomebrew, inspectMise } from "./lib/doctor";
import { formatPlan, planLinkActions } from "./lib/link-dist";

async function main() {
  const homeDir = process.env.HOME;
  if (!homeDir) {
    throw new Error("HOME is not set");
  }

  const repoRoot = process.cwd();
  const sourceRoot = path.join(repoRoot, "dist");
  const { copyPaths, prunePaths, symlinkPaths } = await loadDotfilesConfig(repoRoot, sourceRoot);
  const plan = await planLinkActions({
    copyPaths,
    dryRun: true,
    homeDir,
    prunePaths,
    sourceRoot,
    statePath: deploymentStatePath(homeDir, process.env.XDG_STATE_HOME),
    symlinkPaths,
  });

  const sections: string[] = [];
  const deploymentChanges = plan.actions.filter((action) => action.type !== "noop").length + plan.drifts.length;
  const deploymentFindings = formatPlan(plan)
    .split("\n")
    .filter((line) => line.startsWith("  "))
    .map((line) => line.trim());
  sections.push(formatDoctorSection(
    "deployment",
    deploymentChanges === 0 ? `${plan.actions.length} managed files are converged` : `${deploymentChanges} change or drift item(s) detected`,
    deploymentChanges === 0 ? [] : deploymentFindings,
  ));

  const misePath = path.join(homeDir, ".local", "bin", "mise");
  const miseInventory = inspectMise(await run(misePath, ["ls", "--prunable", "--json"]));
  sections.push(formatDoctorSection(
    "mise",
    `${miseInventory.prunable.length} prunable installation(s)`,
    miseInventory.prunable.length === 0 ? [] : [`prunable: ${miseInventory.prunable.join(", ")}`],
  ));

  const nixPath = await findNix();
  const brewPath = Bun.which("brew");
  if (!nixPath) {
    sections.push(formatDoctorSection("homebrew", "Nix is unavailable", ["nix-darwin declarations cannot be evaluated"]));
  } else {
    const declarations = JSON.parse(await run(nixPath, [
      "--extra-experimental-features",
      "nix-command flakes",
      "eval",
      "--json",
      "--file",
      path.join(repoRoot, "darwin", "homebrew-packages.nix"),
    ])) as { brews: string[]; casks: string[] };
    const declaredCasks = new Set(declarations.casks);
    const declaredFormulae = new Set(declarations.brews);
    const installedFormulae = brewPath ? lines(await run(brewPath, ["leaves"])) : [];
    const installedCasks = brewPath ? lines(await run(brewPath, ["list", "--cask"])) : [];
    const homebrewInventory = inspectHomebrew({
      declaredCasks,
      declaredFormulae,
      installedCasks,
      installedFormulae,
    });
    const homebrewFindings: string[] = [];
    if (!brewPath) {
      homebrewFindings.push("brew is unavailable");
    }
    if (homebrewInventory.missing.length > 0) {
      homebrewFindings.push(`missing: ${homebrewInventory.missing.join(", ")}`);
    }
    if (homebrewInventory.unmanaged.length > 0) {
      homebrewFindings.push(`unmanaged: ${homebrewInventory.unmanaged.join(", ")}`);
    }
    sections.push(formatDoctorSection(
      "homebrew",
      `${declaredCasks.size + declaredFormulae.size} declared, ${installedCasks.length + installedFormulae.length} requested package(s) installed`,
      homebrewFindings,
    ));
  }

  console.log(sections.join("\n\n"));
}

async function exists(filePath: string): Promise<boolean> {
  try {
    await access(filePath);
    return true;
  } catch {
    return false;
  }
}

async function findNix(): Promise<string | null> {
  const fromPath = Bun.which("nix");
  if (fromPath) {
    return fromPath;
  }
  for (const candidate of [
    "/nix/var/nix/profiles/default/bin/nix",
    "/run/current-system/sw/bin/nix",
  ]) {
    if (await exists(candidate)) {
      return candidate;
    }
  }
  return null;
}

async function run(command: string, args: string[]): Promise<string> {
  const result = await runAllowFailure(command, args);
  if (result.code !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed: ${result.stderr.trim()}`);
  }
  return result.stdout;
}

async function runAllowFailure(command: string, args: string[]) {
  const process = Bun.spawn([command, ...args], {
    env: { ...Bun.env, HOMEBREW_NO_AUTO_UPDATE: "1" },
    stderr: "pipe",
    stdout: "pipe",
  });
  const [code, stdout, stderr] = await Promise.all([
    process.exited,
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
  ]);
  return { code, stderr, stdout };
}

function lines(output: string): string[] {
  return output.split("\n").map((line) => line.trim()).filter(Boolean).sort();
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
