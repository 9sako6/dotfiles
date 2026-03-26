import { mkdir, rename, symlink } from "node:fs/promises";
import path from "node:path";
import { backupPathFor, backupRootFor, createTimestamp } from "./backup";
import { lstatOrNull, readDirents, realpathOrNull } from "./fs";
import { sourceToDestinationPath } from "./paths";

export type LinkAction =
  | {
      destinationPath: string;
      sourcePath: string;
      type: "link";
    }
  | {
      backupPath: string;
      destinationPath: string;
      sourcePath: string;
      type: "backup";
    }
  | {
      destinationPath: string;
      sourcePath: string;
      type: "noop";
    };

export type LinkPlan = {
  actions: LinkAction[];
  backupRoot: string;
  dryRun: boolean;
  homeDir: string;
  sourceRoot: string;
  timestamp: string;
};

type PlanOptions = {
  dryRun?: boolean;
  homeDir: string;
  sourceRoot: string;
  timestamp?: string;
};

export async function planLinkActions({
  dryRun = false,
  homeDir,
  sourceRoot,
  timestamp = createTimestamp(),
}: PlanOptions): Promise<LinkPlan> {
  const rootStat = await lstatOrNull(sourceRoot);
  if (!rootStat?.isDirectory()) {
    throw new Error(`dist directory does not exist: ${sourceRoot}`);
  }

  const actions: LinkAction[] = [];
  await planDirectory(sourceRoot, actions, { homeDir, sourceRoot, timestamp });

  return {
    actions,
    backupRoot: backupRootFor(homeDir, timestamp),
    dryRun,
    homeDir,
    sourceRoot,
    timestamp,
  };
}

export async function runLinkPlan(plan: LinkPlan) {
  if (plan.dryRun) {
    return;
  }

  for (const action of plan.actions) {
    if (action.type === "noop") {
      continue;
    }

    if (action.type === "backup") {
      await mkdir(path.dirname(action.backupPath), { recursive: true });
      await rename(action.destinationPath, action.backupPath);
      continue;
    }

    await mkdir(path.dirname(action.destinationPath), { recursive: true });
    await symlink(action.sourcePath, action.destinationPath);
  }
}

export function summarizePlan(plan: LinkPlan) {
  const counts = plan.actions.reduce(
    (result, action) => {
      result[action.type] += 1;
      return result;
    },
    { backup: 0, link: 0, noop: 0 },
  );

  return {
    ...counts,
    backupRoot: plan.backupRoot,
    dryRun: plan.dryRun,
  };
}

async function planDirectory(
  sourcePath: string,
  actions: LinkAction[],
  options: Required<Pick<PlanOptions, "homeDir" | "sourceRoot" | "timestamp">>,
) {
  const entries = await readDirents(sourcePath);

  for (const entry of entries) {
    const childSourcePath = path.join(sourcePath, entry.name);
    if (entry.isDirectory()) {
      await planDirectory(childSourcePath, actions, options);
      continue;
    }
    await planManagedPath(childSourcePath, sourceToDestinationPath(options.sourceRoot, childSourcePath, options.homeDir), actions, options);
  }
}

async function planManagedPath(
  sourcePath: string,
  destinationPath: string,
  actions: LinkAction[],
  options: Required<Pick<PlanOptions, "homeDir" | "sourceRoot" | "timestamp">>,
) {
  const destinationStat = await lstatOrNull(destinationPath);
  if (!destinationStat) {
    actions.push({
      destinationPath,
      sourcePath,
      type: "link",
    });
    return;
  }

  if (destinationStat.isSymbolicLink()) {
    const [resolvedSource, resolvedDestination] = await Promise.all([
      realpathOrNull(sourcePath),
      realpathOrNull(destinationPath),
    ]);

    if (resolvedSource && resolvedDestination && resolvedSource === resolvedDestination) {
      actions.push({
        destinationPath,
        sourcePath,
        type: "noop",
      });
      return;
    }
  }

  actions.push({
    backupPath: backupPathFor(options.homeDir, destinationPath, options.timestamp),
    destinationPath,
    sourcePath,
    type: "backup",
  });
  actions.push({
    destinationPath,
    sourcePath,
    type: "link",
  });
}
