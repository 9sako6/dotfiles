import { access } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { planLinkActions, runLinkPlan } from "./link-dist";

export type SetupStep =
  | {
      kind: "link-dist";
      sourceRoot: string;
    }
  | {
      homeDir: string;
      kind: "install-zinit";
    };

type BuildSetupPlanOptions = {
  homeDir: string;
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
  homeDir,
  repoRoot,
  skipExtraSetup,
  zinitInstalled,
}: BuildSetupPlanOptions): SetupStep[] {
  const steps: SetupStep[] = [
    {
      kind: "link-dist",
      sourceRoot: path.join(repoRoot, "dist"),
    },
  ];

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
    homeDir,
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

    await installZinit(step.homeDir);
  }
}

async function installZinit(homeDir: string) {
  await runShellCommand(
    `HOME="${homeDir}" sh -c "$(curl --fail --show-error --silent --location https://raw.githubusercontent.com/zdharma-continuum/zinit/HEAD/scripts/install.sh)"`,
  );
}

async function runShellCommand(command: string) {
  await new Promise<void>((resolve, reject) => {
    const child = spawn("/bin/sh", ["-c", command], {
      stdio: "inherit",
    });

    child.on("exit", (code) => {
      if (code === 0) {
        resolve();
        return;
      }
      reject(new Error(`command failed with exit code ${code ?? "unknown"}`));
    });
    child.on("error", reject);
  });
}
