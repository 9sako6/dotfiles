import { access, mkdir } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { lstatOrNull } from "./fs";
import { planLinkActions, runLinkPlan } from "./link-dist";

const ZINIT_REPO_URL = "https://github.com/zdharma-continuum/zinit.git";
const ZINIT_REF = "55d19f86f627c9995db9885d0971d9b6701fe0d3";

export type ProcessRunner = (command: string, args: string[]) => Promise<void>;

export type SetupStep =
  | {
      kind: "link-dist";
      sourceRoot: string;
    }
  | {
      brewfilePath: string;
      kind: "install-homebrew-bundle";
    }
  | {
      homeDir: string;
      kind: "install-zinit";
    }
  | {
      distPath: string;
      kind: "sync-skills";
    };

type BuildSetupPlanOptions = {
  brewInstalled: boolean;
  homeDir: string;
  platform: NodeJS.Platform;
  repoRoot: string;
  skipExtraSetup: boolean;
  zinitInstalled: boolean;
};

type RunSetupOptions = {
  homeDir: string;
  repoRoot: string;
  skipExtraSetup?: boolean;
};

export function buildSetupPlan({
  brewInstalled,
  homeDir,
  platform,
  repoRoot,
  skipExtraSetup,
  zinitInstalled,
}: BuildSetupPlanOptions): SetupStep[] {
  const distRoot = path.join(repoRoot, "dist");
  const steps: SetupStep[] = [
    {
      kind: "link-dist",
      sourceRoot: distRoot,
    },
  ];

  if (!skipExtraSetup) {
    steps.push({
      distPath: distRoot,
      kind: "sync-skills",
    });
  }

  if (!skipExtraSetup && platform === "darwin" && brewInstalled) {
    steps.push({
      brewfilePath: path.join(distRoot, ".Brewfile"),
      kind: "install-homebrew-bundle",
    });
  }

  if (!skipExtraSetup && !zinitInstalled) {
    steps.push({
      homeDir,
      kind: "install-zinit",
    });
  }

  return steps;
}

export async function detectZinitInstalled(homeDir: string) {
  try {
    await access(path.join(homeDir, ".local", "share", "zinit", "zinit.git", "zinit.zsh"));
    return true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return false;
    }
    throw error;
  }
}

export async function runSetup({
  homeDir,
  repoRoot,
  skipExtraSetup = process.env.DOTFILES_SKIP_EXTRA_SETUP === "1",
}: RunSetupOptions) {
  const setupPlan = buildSetupPlan({
    brewInstalled: await detectHomebrewInstalled(),
    homeDir,
    platform: process.platform,
    repoRoot,
    skipExtraSetup,
    zinitInstalled: await detectZinitInstalled(homeDir),
  });

  for (const step of setupPlan) {
    if (step.kind === "link-dist") {
      const linkPlan = await planLinkActions({
        homeDir,
        sourceRoot: step.sourceRoot,
      });
      await runLinkPlan(linkPlan);
      continue;
    }

    if (step.kind === "install-homebrew-bundle") {
      await installHomebrewBundle(step.brewfilePath);
      continue;
    }

    await installZinit(step.homeDir);
  }
}

export async function detectHomebrewInstalled() {
  for (const brewBin of ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]) {
    try {
      await access(brewBin);
      return true;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
        throw error;
      }
    }
  }

  return false;
}

export async function installZinit(homeDir: string, runCommand: ProcessRunner = runProcess) {
  const zinitDir = path.join(homeDir, ".local", "share", "zinit", "zinit.git");
  if (await lstatOrNull(zinitDir)) {
    throw new Error(`zinit install target already exists: ${zinitDir}`);
  }
  await mkdir(path.dirname(zinitDir), { recursive: true });
  await runCommand("git", ["init", zinitDir]);
  await runCommand("git", ["-C", zinitDir, "remote", "add", "origin", ZINIT_REPO_URL]);
  await runCommand("git", ["-C", zinitDir, "fetch", "--depth=1", "origin", ZINIT_REF]);
  await runCommand("git", ["-C", zinitDir, "checkout", "--detach", "FETCH_HEAD"]);
}

async function installHomebrewBundle(brewfilePath: string) {
  await runProcess(detectPreferredBrewPath(), ["bundle", `--file=${brewfilePath}`]);
}

function detectPreferredBrewPath() {
  return process.arch === "arm64" ? "/opt/homebrew/bin/brew" : "/usr/local/bin/brew";
}

async function runProcess(command: string, args: string[]) {
  await new Promise<void>((resolve, reject) => {
    const child = spawn(command, args, {
      stdio: "inherit",
    });

    child.on("exit", (code) => {
      if (code === 0) {
        resolve();
        return;
      }
      reject(new Error(`command failed: ${command} ${args.join(" ")} (exit code ${code ?? "unknown"})`));
    });
    child.on("error", reject);
  });
}
