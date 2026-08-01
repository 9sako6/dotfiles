import { createHash } from "node:crypto";
import { mkdir, readFile, readlink, rename, writeFile } from "node:fs/promises";
import path from "node:path";
import { lstatOrNull, readDirents } from "./fs";

export type ManagedDeployment = {
  kind: "copy" | "symlink";
  path: string;
};

export type DeploymentEntry = ManagedDeployment & {
  digest?: string;
};

type DeploymentState = {
  entries: DeploymentEntry[];
  sourceRoot: string;
  version: 1;
};

export type DeploymentDrift = {
  path: string;
  reason: string;
};

export function deploymentStatePath(homeDir: string, stateHome?: string): string {
  return path.join(stateHome ?? path.join(homeDir, ".local", "state"), "dotfiles", "deployment.json");
}

export async function loadDeploymentState(statePath: string): Promise<DeploymentState | null> {
  let raw: string;
  try {
    raw = await readFile(statePath, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return null;
    }
    throw error;
  }

  const parsed = JSON.parse(raw) as Partial<DeploymentState>;
  if (
    parsed.version !== 1 ||
    typeof parsed.sourceRoot !== "string" ||
    !path.isAbsolute(parsed.sourceRoot) ||
    !Array.isArray(parsed.entries)
  ) {
    throw new Error(`invalid deployment state: ${statePath}`);
  }
  const paths = new Set<string>();
  for (const entry of parsed.entries) {
    if (
      !entry ||
      (entry.kind !== "copy" && entry.kind !== "symlink") ||
      typeof entry.path !== "string" ||
      !isSafeRelativePath(entry.path) ||
      (entry.digest !== undefined && typeof entry.digest !== "string")
    ) {
      throw new Error(`invalid deployment state entry: ${statePath}`);
    }
    if (paths.has(entry.path)) {
      throw new Error(`duplicate deployment state entry: ${entry.path}`);
    }
    paths.add(entry.path);
  }

  return parsed as DeploymentState;
}

export async function findOrphanedDeployments(options: {
  currentDeployments: readonly ManagedDeployment[];
  homeDir: string;
  sourceRoot: string;
  statePath: string;
}): Promise<{
  drifts: DeploymentDrift[];
  prunePaths: string[];
  retainedEntries: DeploymentEntry[];
}> {
  const state = await loadDeploymentState(options.statePath);
  const currentPaths = new Set(options.currentDeployments.map((entry) => entry.path));
  const prunePaths = new Set<string>();
  const drifts: DeploymentDrift[] = [];
  const retainedEntries: DeploymentEntry[] = [];

  if (state) {
    for (const entry of state.entries) {
      if (currentPaths.has(entry.path)) {
        continue;
      }
      const classification = await classifyPreviousEntry(
        entry,
        options.homeDir,
        state.sourceRoot,
        options.sourceRoot,
      );
      if (classification === "missing") {
        continue;
      }
      if (classification === "prune") {
        prunePaths.add(entry.path);
        continue;
      }
      drifts.push({ path: entry.path, reason: classification });
      retainedEntries.push(entry);
    }
  }

  if (!state) {
    for (const relativePath of await discoverLegacySymlinks(
      options.homeDir,
      options.sourceRoot,
      currentPaths,
    )) {
      prunePaths.add(relativePath);
    }
  }

  return {
    drifts: drifts.sort((left, right) => left.path.localeCompare(right.path)),
    prunePaths: [...prunePaths].sort(),
    retainedEntries: retainedEntries.sort((left, right) => left.path.localeCompare(right.path)),
  };
}

export async function writeDeploymentState(options: {
  currentDeployments: readonly ManagedDeployment[];
  homeDir: string;
  retainedEntries: readonly DeploymentEntry[];
  sourceRoot: string;
  statePath: string;
}): Promise<void> {
  const entriesByPath = new Map<string, DeploymentEntry>();
  for (const entry of options.retainedEntries) {
    entriesByPath.set(entry.path, entry);
  }
  for (const deployment of options.currentDeployments) {
    const destinationPath = path.join(options.homeDir, deployment.path);
    const entry: DeploymentEntry = { ...deployment };
    if (deployment.kind === "copy") {
      entry.digest = await digestFile(destinationPath);
    }
    entriesByPath.set(entry.path, entry);
  }

  const state: DeploymentState = {
    entries: [...entriesByPath.values()].sort((left, right) => left.path.localeCompare(right.path)),
    sourceRoot: options.sourceRoot,
    version: 1,
  };
  const stateDir = path.dirname(options.statePath);
  const tempPath = path.join(stateDir, `.deployment.${process.pid}.${Date.now()}.tmp`);
  await mkdir(stateDir, { recursive: true });
  await writeFile(tempPath, `${JSON.stringify(state, null, 2)}\n`, { mode: 0o600 });
  await rename(tempPath, options.statePath);
}

async function classifyPreviousEntry(
  entry: DeploymentEntry,
  homeDir: string,
  previousSourceRoot: string,
  sourceRoot: string,
): Promise<"missing" | "prune" | string> {
  const destinationPath = path.join(homeDir, entry.path);
  const stat = await lstatOrNull(destinationPath);
  if (!stat) {
    return "missing";
  }

  if (entry.kind === "symlink") {
    if (!stat.isSymbolicLink()) {
      return "previously managed symlink was replaced";
    }
    const targetPath = await absoluteSymlinkTarget(destinationPath);
    if (isWithin(previousSourceRoot, targetPath) || isWithin(sourceRoot, targetPath)) {
      return "prune";
    }
    return "previously managed symlink points outside this dotfiles repository";
  }

  if (!stat.isFile() || stat.isSymbolicLink()) {
    return "previously managed copy changed type";
  }
  if (!entry.digest || await digestFile(destinationPath) !== entry.digest) {
    return "previously managed copy was modified";
  }
  return "prune";
}

async function discoverLegacySymlinks(
  homeDir: string,
  sourceRoot: string,
  currentPaths: ReadonlySet<string>,
): Promise<string[]> {
  const found = new Set<string>();
  if (!(await lstatOrNull(homeDir))) {
    return [];
  }
  const homeEntries = await readDirents(homeDir);
  for (const entry of homeEntries) {
    const relativePath = entry.name;
    const destinationPath = path.join(homeDir, relativePath);
    if (entry.isSymbolicLink()) {
      await collectLegacySymlink(destinationPath, relativePath, sourceRoot, currentPaths, found);
    }
  }

  const managedRoots = new Set([...currentPaths].map((relativePath) => relativePath.split(path.sep)[0]));
  for (const managedRoot of managedRoots) {
    const rootPath = path.join(homeDir, managedRoot);
    const stat = await lstatOrNull(rootPath);
    if (stat?.isDirectory() && !stat.isSymbolicLink()) {
      await scanDirectory(rootPath, managedRoot, sourceRoot, currentPaths, found);
    }
  }

  return [...found].sort();
}

async function scanDirectory(
  directoryPath: string,
  relativeDirectory: string,
  sourceRoot: string,
  currentPaths: ReadonlySet<string>,
  found: Set<string>,
): Promise<void> {
  for (const entry of await readDirents(directoryPath)) {
    const destinationPath = path.join(directoryPath, entry.name);
    const relativePath = path.join(relativeDirectory, entry.name);
    if (entry.isSymbolicLink()) {
      await collectLegacySymlink(destinationPath, relativePath, sourceRoot, currentPaths, found);
    } else if (entry.isDirectory()) {
      await scanDirectory(destinationPath, relativePath, sourceRoot, currentPaths, found);
    }
  }
}

async function collectLegacySymlink(
  destinationPath: string,
  relativePath: string,
  sourceRoot: string,
  currentPaths: ReadonlySet<string>,
  found: Set<string>,
): Promise<void> {
  if (isCurrentDeploymentOrParent(relativePath, currentPaths)) {
    return;
  }
  if (isWithin(sourceRoot, await absoluteSymlinkTarget(destinationPath))) {
    found.add(relativePath);
  }
}

function isCurrentDeploymentOrParent(
  relativePath: string,
  currentPaths: ReadonlySet<string>,
): boolean {
  if (currentPaths.has(relativePath)) {
    return true;
  }
  const prefix = `${relativePath}${path.sep}`;
  return [...currentPaths].some((currentPath) => currentPath.startsWith(prefix));
}

async function absoluteSymlinkTarget(linkPath: string): Promise<string> {
  return path.resolve(path.dirname(linkPath), await readlink(linkPath));
}

function isWithin(rootPath: string, targetPath: string): boolean {
  const relativePath = path.relative(rootPath, targetPath);
  return relativePath === "" || (!relativePath.startsWith("..") && !path.isAbsolute(relativePath));
}

function isSafeRelativePath(relativePath: string): boolean {
  return relativePath !== "" && relativePath !== "." && !path.isAbsolute(relativePath) &&
    !relativePath.split(/[\\/]/).includes("..");
}

async function digestFile(filePath: string): Promise<string> {
  return createHash("sha256").update(await readFile(filePath)).digest("hex");
}
