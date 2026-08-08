#!/usr/bin/env bun

import { access, realpath } from "node:fs/promises";
import { userInfo } from "node:os";
import path from "node:path";
import {
  type DoctorSectionContent,
  inspectHomebrew,
  inspectMise,
  inspectSystem,
  parseHomebrewDeclarationNames,
  runDoctor,
} from "../lib/doctor";
import { resolveRepoRoot } from "../lib/paths";
import {
  inspectSelectedSystemSource,
  systemSourceDataRoot,
} from "../lib/system-source";

async function main() {
  const homeDir = process.env.HOME;
  if (!homeDir) {
    throw new Error("HOME is not set");
  }

  const repoRoot = await resolveRepoRoot(import.meta.path);
  const report = await runDoctor([
    {
      inspect: () => inspectMiseInstallations(homeDir),
      title: "mise",
    },
    {
      inspect: () => inspectSystemConfiguration(repoRoot, homeDir),
      title: "system",
    },
    {
      inspect: () => inspectHomebrewInstallations(repoRoot, homeDir),
      title: "homebrew",
    },
  ]);
  console.log(report.output);
  if (report.failed) {
    process.exitCode = 1;
  }
}

async function inspectSystemConfiguration(
  repoRoot: string,
  homeDir: string,
): Promise<DoctorSectionContent> {
  const nixPath = await findNix();
  if (!nixPath) {
    return {
      findings: ["system declarations cannot be evaluated"],
      nextSteps: ["mise run apply"],
      summary: "Nix is unavailable",
    };
  }

  const source = await inspectSelectedSystemSource({
    dataRoot: systemSourceDataRoot(homeDir, Bun.env.XDG_DATA_HOME),
    publicDirectory: repoRoot,
    selectionPath: "/etc/nix-darwin/flake.nix",
  });
  const nixArgs = [
    "--extra-experimental-features",
    "nix-command flakes",
    "eval",
    "--raw",
  ];
  if (source.kind === "default") nixArgs.push("--impure");
  nixArgs.push(`path:${source.directory}#darwinConfigurations.current.system`);
  const expectedStorePath = (await run(
    nixPath,
    nixArgs,
    source.kind === "default" ? {
      DARWIN_PRIMARY_USER: userInfo().username,
      DOTFILES_DIR: repoRoot,
    } : undefined,
  )).trim();
  const currentSystem = "/run/current-system";
  const activeStorePath = await exists(currentSystem) ? await realpath(currentSystem) : null;
  const inventory = inspectSystem({ activeStorePath, expectedStorePath });

  if (inventory.status === "active") {
    return {
      findings: [],
      nextSteps: [],
      summary: "system configuration is active",
    };
  }
  return {
    findings: [inventory.status === "missing"
      ? "no active nix-darwin system was found"
      : "active system does not match repository declarations"],
    nextSteps: ["mise run apply"],
    summary: "system configuration is not active",
  };
}

async function inspectMiseInstallations(homeDir: string): Promise<DoctorSectionContent> {
  const misePath = path.join(homeDir, ".local", "bin", "mise");
  const [missingRaw, prunableRaw] = await Promise.all([
    run(misePath, ["ls", "--missing", "--json"]),
    run(misePath, ["ls", "--prunable", "--json"]),
  ]);
  const miseInventory = inspectMise({ missingRaw, prunableRaw });
  const findings: string[] = [];
  if (miseInventory.missing.length > 0) {
    findings.push(`missing: ${miseInventory.missing.join(", ")}`);
  }
  if (miseInventory.prunable.length > 0) {
    findings.push(`prunable: ${miseInventory.prunable.join(", ")}`);
  }
  return {
    findings,
    nextSteps: miseInventory.prunable.length > 0 ? ["mise prune --tools"] : [],
    summary:
      `${miseInventory.missing.length} missing, ${miseInventory.prunable.length} prunable installation(s)`,
  };
}

async function inspectHomebrewInstallations(
  repoRoot: string,
  homeDir: string,
): Promise<DoctorSectionContent> {
  const nixPath = await findNix();
  const brewPath = Bun.which("brew");
  if (!nixPath) {
    return {
      findings: ["nix-darwin declarations cannot be evaluated"],
      nextSteps: [],
      summary: "Nix is unavailable",
    };
  }

  const source = await inspectSelectedSystemSource({
    dataRoot: systemSourceDataRoot(homeDir, Bun.env.XDG_DATA_HOME),
    publicDirectory: repoRoot,
    selectionPath: "/etc/nix-darwin/flake.nix",
  });
  const evaluate = async (option: "brews" | "casks") => {
    const args = [
      "--extra-experimental-features",
      "nix-command flakes",
      "eval",
      "--json",
    ];
    if (source.kind === "default") args.push("--impure");
    args.push(
      `path:${source.directory}#darwinConfigurations.current.config.homebrew.${option}`,
    );
    return parseHomebrewDeclarationNames(await run(
      nixPath,
      args,
      source.kind === "default" ? {
        DARWIN_PRIMARY_USER: userInfo().username,
        DOTFILES_DIR: repoRoot,
      } : undefined,
    ));
  };
  const [declaredCasks, declaredFormulae] = await Promise.all([
    evaluate("casks"),
    evaluate("brews"),
  ]);
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
  return {
    findings: homebrewFindings,
    nextSteps: [],
    summary:
      `${declaredCasks.size + declaredFormulae.size} declared, ${installedCasks.length + installedFormulae.length} requested package(s) installed`,
  };
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

async function run(
  command: string,
  args: string[],
  env?: Record<string, string>,
): Promise<string> {
  const result = await runAllowFailure(command, args, env);
  if (result.code !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed: ${result.stderr.trim()}`);
  }
  return result.stdout;
}

async function runAllowFailure(command: string, args: string[], env?: Record<string, string>) {
  const process = Bun.spawn([command, ...args], {
    env: { ...Bun.env, ...env, HOMEBREW_NO_AUTO_UPDATE: "1" },
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
