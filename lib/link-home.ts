import { copyFile, mkdir, readFile, rename, rm, symlink } from "node:fs/promises";
import path from "node:path";
import {
  allocateBackupRoot,
  backupPathFor,
  backupRootFor,
  createTimestamp,
  removeEmptyBackupRoot,
  reserveBackupRoot,
} from "./backup";
import {
  findOrphanedDeployments,
  writeDeploymentState,
  type DeploymentEntry,
  type ManagedDeployment,
} from "./deployment-state";
import { lstatOrNull, readDirents, realpathOrNull } from "./fs";
import { sourceToDestinationPath } from "./paths";

export type LinkAction =
  | {
      destinationPath: string;
      sourcePath: string;
      type: "link";
    }
  | {
      destinationPath: string;
      sourcePath: string;
      type: "copy";
    }
  | {
      backupPath: string;
      destinationPath: string;
      replacementType: "copy" | "link";
      sourcePath: string;
      type: "replace";
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
    }
  | {
      backupPath: string;
      destinationPath: string;
      type: "prune";
    };

export type LinkDrift = {
  destinationPath: string;
  reason: string;
};

export type LinkPlan = {
  actions: LinkAction[];
  backupRoot: string;
  deploymentState: {
    currentDeployments: ManagedDeployment[];
    retainedEntries: DeploymentEntry[];
    statePath: string;
  };
  drifts: LinkDrift[];
  homeDir: string;
  sourceRoot: string;
  timestamp: string;
};

type PlanOptions = {
  copyPaths?: ReadonlySet<string>;
  homeDir: string;
  prunePaths?: ReadonlySet<string>;
  sourceRoot: string;
  statePath: string;
  symlinkPaths: ReadonlySet<string>;
  timestamp?: string;
};

export async function planLinkActions({
  copyPaths = new Set(),
  homeDir,
  prunePaths = new Set(),
  sourceRoot,
  statePath,
  symlinkPaths,
  timestamp = createTimestamp(),
}: PlanOptions): Promise<LinkPlan> {
  const rootStat = await lstatOrNull(sourceRoot);
  if (!rootStat?.isDirectory()) {
    throw new Error(`home directory does not exist: ${sourceRoot}`);
  }

  const actions: LinkAction[] = [];
  const drifts: LinkDrift[] = [];
  const currentDeployments: ManagedDeployment[] = [];
  await planDirectory(sourceRoot, actions, {
    copyPaths,
    currentDeployments,
    homeDir,
    sourceRoot,
    symlinkPaths,
    timestamp,
  });
  await planPruneActions(actions, {
    homeDir,
    prunePaths,
    sourceRoot,
    timestamp,
  });

  const orphaned = await findOrphanedDeployments({
    currentDeployments,
    homeDir,
    sourceRoot,
    statePath,
  });
  for (const relativePath of orphaned.prunePaths) {
    const destinationPath = path.join(homeDir, relativePath);
    if (!actions.some((action) => pathsOverlap(action.destinationPath, destinationPath))) {
      actions.push({
        backupPath: backupPathFor(homeDir, destinationPath, timestamp),
        destinationPath,
        type: "prune",
      });
    }
  }
  for (const drift of orphaned.drifts) {
    const destinationPath = path.join(homeDir, drift.path);
    if (!actions.some((action) => pathsOverlap(action.destinationPath, destinationPath))) {
      drifts.push({ destinationPath, reason: drift.reason });
    }
  }
  const plannedRemovals = actions
    .filter((action) => action.type === "prune")
    .map((action) => action.destinationPath);
  const deploymentState = {
    currentDeployments,
    retainedEntries: orphaned.retainedEntries.filter((entry) =>
      !plannedRemovals.some((destinationPath) =>
        pathsOverlap(destinationPath, path.join(homeDir, entry.path)),
      )
    ),
    statePath,
  };
  const hasBackups = actions.some((action) => "backupPath" in action);
  const backupRoot = hasBackups
    ? await allocateBackupRoot(homeDir, timestamp)
    : backupRootFor(homeDir, timestamp);
  for (const action of actions) {
    if ("backupPath" in action) {
      action.backupPath = path.join(backupRoot, path.relative(homeDir, action.destinationPath));
    }
  }

  return {
    actions,
    backupRoot,
    deploymentState,
    drifts,
    homeDir,
    sourceRoot,
    timestamp,
  };
}

function pathsOverlap(left: string, right: string): boolean {
  return isPathWithin(left, right) || isPathWithin(right, left);
}

function isPathWithin(ancestor: string, target: string): boolean {
  const relativePath = path.relative(ancestor, target);
  return relativePath === "" || (!relativePath.startsWith("..") && !path.isAbsolute(relativePath));
}

export async function runLinkPlan(plan: LinkPlan) {
  if (plan.actions.some((action) => "backupPath" in action)) {
    await reserveBackupRoot(plan.backupRoot);
  }

  const rollbackActions: RollbackAction[] = [];
  try {
    for (const action of plan.actions) {
      if (action.type === "noop") {
        continue;
      }

      if (action.type === "replace") {
        await replaceWithBackup(action);
        rollbackActions.push({
          backupPath: action.backupPath,
          destinationPath: action.destinationPath,
          type: "restore",
        });
        continue;
      }

      if (action.type === "backup") {
        await mkdir(path.dirname(action.backupPath), { recursive: true });
        await rename(action.destinationPath, action.backupPath);
        rollbackActions.push({
          backupPath: action.backupPath,
          destinationPath: action.destinationPath,
          type: "restore",
        });
        continue;
      }

      if (action.type === "prune") {
        await mkdir(path.dirname(action.backupPath), { recursive: true });
        await rename(action.destinationPath, action.backupPath);
        rollbackActions.push({
          backupPath: action.backupPath,
          destinationPath: action.destinationPath,
          type: "restore",
        });
        continue;
      }

      await mkdir(path.dirname(action.destinationPath), { recursive: true });
      if (action.type === "copy") {
        await copyFile(action.sourcePath, action.destinationPath);
      } else {
        await symlink(action.sourcePath, action.destinationPath);
      }
      rollbackActions.push({ destinationPath: action.destinationPath, type: "remove" });
    }

    await writeDeploymentState({
      ...plan.deploymentState,
      homeDir: plan.homeDir,
      sourceRoot: plan.sourceRoot,
    });
  } catch (error) {
    const rollbackErrors = await rollbackLinkActions(rollbackActions);
    await removeEmptyBackupRoot(plan.backupRoot);
    if (rollbackErrors.length > 0) {
      throw new AggregateError(
        [error, ...rollbackErrors],
        "link plan failed and rollback was incomplete",
      );
    }
    throw error;
  }
}

type RollbackAction =
  | {
      destinationPath: string;
      type: "remove";
    }
  | {
      backupPath: string;
      destinationPath: string;
      type: "restore";
    };

async function rollbackLinkActions(actions: readonly RollbackAction[]): Promise<unknown[]> {
  const errors: unknown[] = [];
  for (const action of [...actions].reverse()) {
    try {
      if (action.type === "remove") {
        await rm(action.destinationPath, { force: true, recursive: true });
      } else {
        await rm(action.destinationPath, { force: true, recursive: true });
        await mkdir(path.dirname(action.destinationPath), { recursive: true });
        await rename(action.backupPath, action.destinationPath);
      }
    } catch (error) {
      errors.push(error);
    }
  }
  return errors;
}

type ReplaceAction = Extract<LinkAction, { type: "replace" }>;

async function replaceWithBackup(action: ReplaceAction) {
  const destinationDir = path.dirname(action.destinationPath);
  const tempPath = path.join(
    destinationDir,
    `.dotfiles-${path.basename(action.destinationPath)}.${process.pid}.${Date.now()}.tmp`,
  );

  await mkdir(destinationDir, { recursive: true });
  await mkdir(path.dirname(action.backupPath), { recursive: true });
  try {
    if (action.replacementType === "copy") {
      await copyFile(action.sourcePath, tempPath);
    } else {
      await symlink(action.sourcePath, tempPath);
    }
    await rename(action.destinationPath, action.backupPath);
    try {
      await rename(tempPath, action.destinationPath);
    } catch (error) {
      if (!(await lstatOrNull(action.destinationPath))) {
        await rename(action.backupPath, action.destinationPath);
      }
      throw error;
    }
  } finally {
    await rm(tempPath, { force: true, recursive: true });
  }
}

export function formatPlan(plan: LinkPlan): string {
  const summary = summarizeLinkPlan(plan);
  const lines = summary.findings.map((finding) => `  ${finding}`);
  const { counts } = summary;
  const parts: string[] = [];
  if (counts.link > 0) parts.push(`${counts.link} link`);
  if (counts.copy > 0) parts.push(`${counts.copy} copy`);
  if (counts.prune > 0) parts.push(`${counts.prune} prune`);
  if (counts.backup > 0) parts.push(`${counts.backup} backup`);
  if (counts.drift > 0) parts.push(`${counts.drift} drift`);
  if (counts.noop > 0) parts.push(`${counts.noop} unchanged`);

  if (lines.length > 0) {
    lines.push("");
  }
  lines.push(parts.join(", "));

  return lines.join("\n");
}

export function summarizeLinkPlan(plan: LinkPlan): {
  changeCount: number;
  counts: { backup: number; copy: number; drift: number; link: number; noop: number; prune: number };
  findings: string[];
  managedCount: number;
} {
  const findings: string[] = [];
  const counts = { backup: 0, copy: 0, link: 0, noop: 0, prune: 0 };
  const repoRoot = path.dirname(plan.sourceRoot);

  for (const action of plan.actions) {
    if (action.type === "replace") {
      counts.backup += 1;
      counts[action.replacementType] += 1;
    } else {
      counts[action.type] += 1;
    }
    if (action.type === "noop") continue;

    if (action.type === "replace") {
      findings.push(`backup  ${tildefy(action.destinationPath, plan.homeDir)} → ${tildefy(action.backupPath, plan.homeDir)}`);
      findings.push(`${action.replacementType.padEnd(8)}${path.relative(repoRoot, action.sourcePath)} → ${tildefy(action.destinationPath, plan.homeDir)}`);
    } else if (action.type === "backup") {
      findings.push(`backup  ${tildefy(action.destinationPath, plan.homeDir)} → ${tildefy(action.backupPath, plan.homeDir)}`);
    } else if (action.type === "prune") {
      findings.push(`prune   ${tildefy(action.destinationPath, plan.homeDir)} → ${tildefy(action.backupPath, plan.homeDir)}`);
    } else {
      findings.push(`${action.type.padEnd(8)}${path.relative(repoRoot, action.sourcePath)} → ${tildefy(action.destinationPath, plan.homeDir)}`);
    }
  }

  for (const drift of plan.drifts) {
    findings.push(`drift   ${tildefy(drift.destinationPath, plan.homeDir)} (${drift.reason})`);
  }

  return {
    changeCount: plan.actions.filter((action) => action.type !== "noop").length +
      plan.drifts.length,
    counts: { ...counts, drift: plan.drifts.length },
    findings,
    managedCount: plan.actions.length,
  };
}

async function planPruneActions(
  actions: LinkAction[],
  options: Required<Pick<PlanOptions, "homeDir" | "prunePaths" | "sourceRoot" | "timestamp">>,
) {
  for (const relativePath of options.prunePaths) {
    const sourcePath = path.join(options.sourceRoot, relativePath);
    const destinationPath = sourceToDestinationPath(options.sourceRoot, sourcePath, options.homeDir);
    await planPrunePath(sourcePath, destinationPath, actions, options);
  }
}

async function planPrunePath(
  sourcePath: string,
  destinationPath: string,
  actions: LinkAction[],
  options: Required<Pick<PlanOptions, "homeDir" | "timestamp">>,
) {
  const destinationStat = await lstatOrNull(destinationPath);
  if (!destinationStat) {
    return;
  }

  const sourceStat = await lstatOrNull(sourcePath);
  if (!sourceStat) {
    actions.push({
      backupPath: backupPathFor(options.homeDir, destinationPath, options.timestamp),
      destinationPath,
      type: "prune",
    });
    return;
  }

  if (!sourceStat.isDirectory() || !destinationStat.isDirectory()) {
    return;
  }

  const entries = await readDirents(destinationPath);
  for (const entry of entries) {
    await planPrunePath(
      path.join(sourcePath, entry.name),
      path.join(destinationPath, entry.name),
      actions,
      options,
    );
  }
}

function tildefy(filePath: string, homeDir: string): string {
  if (filePath.startsWith(homeDir)) {
    return "~" + filePath.slice(homeDir.length);
  }
  return filePath;
}

function isManagedTarget(relativePath: string, managedPaths: ReadonlySet<string>): boolean {
  if (managedPaths.has(relativePath)) return true;
  let dir = path.dirname(relativePath);
  while (dir !== ".") {
    if (managedPaths.has(dir)) return true;
    dir = path.dirname(dir);
  }
  return false;
}

async function planDirectory(
  sourcePath: string,
  actions: LinkAction[],
  options: Required<
    Pick<PlanOptions, "copyPaths" | "homeDir" | "sourceRoot" | "symlinkPaths" | "timestamp"> & {
      currentDeployments: ManagedDeployment[];
    }
  >,
  treatDescendantsAsMissing = false,
) {
  let nextTreatDescendantsAsMissing = treatDescendantsAsMissing;
  if (!treatDescendantsAsMissing && sourcePath !== options.sourceRoot) {
    const destinationPath = sourceToDestinationPath(options.sourceRoot, sourcePath, options.homeDir);
    const destinationStat = await lstatOrNull(destinationPath);
    if (destinationStat && !destinationStat.isDirectory()) {
      actions.push({
        backupPath: backupPathFor(options.homeDir, destinationPath, options.timestamp),
        destinationPath,
        sourcePath,
        type: "backup",
      });
      nextTreatDescendantsAsMissing = true;
    }
  }

  const entries = await readDirents(sourcePath);

  for (const entry of entries) {
    const childSourcePath = path.join(sourcePath, entry.name);
    if (entry.isDirectory()) {
      await planDirectory(childSourcePath, actions, options, nextTreatDescendantsAsMissing);
      continue;
    }
    const relativePath = path.relative(options.sourceRoot, childSourcePath);
    const isCopyTargetFile = isManagedTarget(relativePath, options.copyPaths);
    const isSymlinkTargetFile = isManagedTarget(relativePath, options.symlinkPaths);
    if (!isCopyTargetFile && !isSymlinkTargetFile) {
      continue;
    }
    options.currentDeployments.push({
      kind: isCopyTargetFile ? "copy" : "symlink",
      path: relativePath,
    });
    await planManagedPath(
      childSourcePath,
      sourceToDestinationPath(options.sourceRoot, childSourcePath, options.homeDir),
      actions,
      options,
      nextTreatDescendantsAsMissing,
      isCopyTargetFile,
    );
  }
}

async function planManagedPath(
  sourcePath: string,
  destinationPath: string,
  actions: LinkAction[],
  options: Required<Pick<PlanOptions, "homeDir" | "sourceRoot" | "timestamp">>,
  treatDestinationAsMissing = false,
  isCopyTargetFile = false,
) {
  const actionType = isCopyTargetFile ? "copy" : "link";

  if (treatDestinationAsMissing) {
    actions.push({ destinationPath, sourcePath, type: actionType });
    return;
  }

  const destinationStat = await lstatOrNull(destinationPath);
  if (!destinationStat) {
    actions.push({ destinationPath, sourcePath, type: actionType });
    return;
  }

  if (isCopyTargetFile) {
    if (destinationStat.isFile() && !destinationStat.isSymbolicLink()) {
      const [sourceContent, destContent] = await Promise.all([
        readFile(sourcePath),
        readFile(destinationPath),
      ]);
      if (sourceContent.equals(destContent)) {
        actions.push({ destinationPath, sourcePath, type: "noop" });
        return;
      }
    }
  } else if (destinationStat.isSymbolicLink()) {
    const [resolvedSource, resolvedDestination] = await Promise.all([
      realpathOrNull(sourcePath),
      realpathOrNull(destinationPath),
    ]);
    if (resolvedSource && resolvedDestination && resolvedSource === resolvedDestination) {
      actions.push({ destinationPath, sourcePath, type: "noop" });
      return;
    }
  }

  actions.push({
    backupPath: backupPathFor(options.homeDir, destinationPath, options.timestamp),
    destinationPath,
    replacementType: actionType,
    sourcePath,
    type: "replace",
  });
}
