import { copyFile, mkdir, readFile, rename, rm, symlink } from "node:fs/promises";
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
      destinationPath: string;
      sourcePath: string;
      type: "copy";
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

export type LinkPlan = {
  actions: LinkAction[];
  backupRoot: string;
  dryRun: boolean;
  homeDir: string;
  sourceRoot: string;
  timestamp: string;
};

type PlanOptions = {
  copyPaths?: ReadonlySet<string>;
  dryRun?: boolean;
  homeDir: string;
  prunePaths?: ReadonlySet<string>;
  sourceRoot: string;
  symlinkPaths: ReadonlySet<string>;
  timestamp?: string;
};

export async function planLinkActions({
  copyPaths = new Set(),
  dryRun = false,
  homeDir,
  prunePaths = new Set(),
  sourceRoot,
  symlinkPaths,
  timestamp = createTimestamp(),
}: PlanOptions): Promise<LinkPlan> {
  const rootStat = await lstatOrNull(sourceRoot);
  if (!rootStat?.isDirectory()) {
    throw new Error(`dist directory does not exist: ${sourceRoot}`);
  }

  const actions: LinkAction[] = [];
  await planDirectory(sourceRoot, actions, {
    copyPaths,
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

  for (let index = 0; index < plan.actions.length; index += 1) {
    const action = plan.actions[index];
    if (action.type === "noop") {
      continue;
    }

    if (action.type === "backup") {
      const replacement = plan.actions[index + 1];
      if (isReplacementForBackup(action, replacement)) {
        await replaceWithBackup(action, replacement);
        index += 1;
        continue;
      }
      await mkdir(path.dirname(action.backupPath), { recursive: true });
      await rename(action.destinationPath, action.backupPath);
      continue;
    }

    if (action.type === "prune") {
      await mkdir(path.dirname(action.backupPath), { recursive: true });
      await rename(action.destinationPath, action.backupPath);
      continue;
    }

    await mkdir(path.dirname(action.destinationPath), { recursive: true });
    if (action.type === "copy") {
      await copyFile(action.sourcePath, action.destinationPath);
    } else {
      await symlink(action.sourcePath, action.destinationPath);
    }
  }
}

type ReplacementAction = Extract<LinkAction, { type: "copy" | "link" }>;
type BackupAction = Extract<LinkAction, { type: "backup" }>;

function isReplacementForBackup(
  backup: BackupAction,
  replacement: LinkAction | undefined,
): replacement is ReplacementAction {
  return (
    !!replacement &&
    (replacement.type === "copy" || replacement.type === "link") &&
    replacement.destinationPath === backup.destinationPath
  );
}

async function replaceWithBackup(backup: BackupAction, replacement: ReplacementAction) {
  const destinationDir = path.dirname(replacement.destinationPath);
  const tempPath = path.join(
    destinationDir,
    `.dotfiles-${path.basename(replacement.destinationPath)}.${process.pid}.${Date.now()}.tmp`,
  );

  await mkdir(destinationDir, { recursive: true });
  await mkdir(path.dirname(backup.backupPath), { recursive: true });
  try {
    if (replacement.type === "copy") {
      await copyFile(replacement.sourcePath, tempPath);
    } else {
      await symlink(replacement.sourcePath, tempPath);
    }
    await rename(backup.destinationPath, backup.backupPath);
    try {
      await rename(tempPath, replacement.destinationPath);
    } catch (error) {
      if (!(await lstatOrNull(replacement.destinationPath))) {
        await rename(backup.backupPath, backup.destinationPath);
      }
      throw error;
    }
  } finally {
    await rm(tempPath, { force: true, recursive: true });
  }
}

export function formatPlan(plan: LinkPlan): string {
  const lines: string[] = [];
  const counts = { backup: 0, copy: 0, link: 0, noop: 0, prune: 0 };
  const repoRoot = path.dirname(plan.sourceRoot);

  for (const action of plan.actions) {
    counts[action.type] += 1;
    if (action.type === "noop") continue;

    if (action.type === "backup") {
      lines.push(`  backup  ${tildefy(action.destinationPath, plan.homeDir)} → ${tildefy(action.backupPath, plan.homeDir)}`);
    } else if (action.type === "prune") {
      lines.push(`  prune   ${tildefy(action.destinationPath, plan.homeDir)} → ${tildefy(action.backupPath, plan.homeDir)}`);
    } else {
      lines.push(`  ${action.type.padEnd(8)}${path.relative(repoRoot, action.sourcePath)} → ${tildefy(action.destinationPath, plan.homeDir)}`);
    }
  }

  const parts: string[] = [];
  if (counts.link > 0) parts.push(`${counts.link} link`);
  if (counts.copy > 0) parts.push(`${counts.copy} copy`);
  if (counts.prune > 0) parts.push(`${counts.prune} prune`);
  if (counts.backup > 0) parts.push(`${counts.backup} backup`);
  if (counts.noop > 0) parts.push(`${counts.noop} unchanged`);

  if (lines.length > 0) {
    lines.push("");
  }
  lines.push(parts.join(", "));

  return lines.join("\n");
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
    Pick<PlanOptions, "copyPaths" | "homeDir" | "sourceRoot" | "symlinkPaths" | "timestamp">
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
    sourcePath,
    type: "backup",
  });
  actions.push({ destinationPath, sourcePath, type: actionType });
}
