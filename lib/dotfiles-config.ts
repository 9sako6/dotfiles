import { readFile } from "node:fs/promises";
import path from "node:path";
import { lstatOrNull } from "./fs";

export type DotfilesConfig = {
  symlinkPaths: ReadonlySet<string>;
  copyPaths: ReadonlySet<string>;
  prunePaths: ReadonlySet<string>;
};

const managedFields = ["symlink", "copy", "prune"] as const;
type ManagedFields = (typeof managedFields)[number];

export async function loadDotfilesConfig(
  repoRoot: string,
  sourceRoot: string,
): Promise<DotfilesConfig> {
  const configPath = path.join(repoRoot, ".dotfiles.json");
  const parsed = parseDotfilesConfig(await readConfigFile(configPath));
  const symlinkPaths = new Set(parsed.symlink);
  const copyPaths = new Set(parsed.copy);
  const prunePaths = new Set(parsed.prune);
  validateNoConflicts(symlinkPaths, copyPaths);
  await validatePrunePaths(sourceRoot, copyPaths, prunePaths);
  await validatePathsExist(sourceRoot, symlinkPaths, "symlink");
  await validatePathsExist(sourceRoot, copyPaths, "copy");
  return { symlinkPaths, copyPaths, prunePaths };
}

async function readConfigFile(configPath: string): Promise<string> {
  try {
    return await readFile(configPath, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      throw new Error(`.dotfiles.json: file not found: ${configPath}`);
    }
    throw error;
  }
}

class DotfilesConfigError extends Error {}

function parseDotfilesConfig(raw: string): Record<ManagedFields, string[]> {
  assertNoDuplicateMembers(raw);
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch (error) {
    throw new Error(
      `.dotfiles.json: invalid JSON: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(".dotfiles.json: root must be an object");
  }
  const root = value as Record<string, unknown>;
  const unknownKeys = Object.keys(root).filter(
    (key) => !(managedFields as readonly string[]).includes(key),
  );
  if (unknownKeys.length > 0) {
    throw new Error(
      `.dotfiles.json: unknown key(s): ${unknownKeys.join(", ")}; allowed: ${managedFields.join(", ")}`,
    );
  }
  return {
    symlink: parseManagedPaths(root, "symlink"),
    copy: parseManagedPaths(root, "copy"),
    prune: parseManagedPaths(root, "prune"),
  };
}

function assertNoDuplicateMembers(raw: string): void {
  const memberKeySets: Set<string>[] = [];
  for (let index = 0; index < raw.length; index++) {
    const char = raw[index]!;
    if (char === '"') {
      const keyEnd = scanStringEnd(raw, index);
      let next = keyEnd + 1;
      while (next < raw.length && isJsonWhitespace(raw[next]!)) {
        next += 1;
      }
      const parent = memberKeySets[memberKeySets.length - 1];
      if (parent && raw[next] === ":") {
        const key = JSON.parse(raw.slice(index, keyEnd + 1)) as string;
        if (parent.has(key)) {
          throw new DotfilesConfigError(`.dotfiles.json: duplicate member "${key}"`);
        }
        parent.add(key);
      }
      index = keyEnd;
      continue;
    }
    if (char === "{") {
      memberKeySets.push(new Set());
    } else if (char === "}") {
      memberKeySets.pop();
    }
  }
}

function scanStringEnd(raw: string, start: number): number {
  for (let index = start + 1; index < raw.length; index++) {
    const char = raw[index]!;
    if (char === "\\") {
      index += 1;
    } else if (char === '"') {
      return index;
    }
  }
  return raw.length - 1;
}

function isJsonWhitespace(char: string): boolean {
  return char === " " || char === "\t" || char === "\n" || char === "\r";
}

function parseManagedPaths(root: Record<string, unknown>, fieldName: string): string[] {
  const value = root[fieldName];
  if (value === undefined) {
    return [];
  }
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== "string")) {
    throw new Error(`.dotfiles.json: "${fieldName}" must be an array of strings`);
  }
  const entries = value as string[];
  for (const entry of entries) {
    validateManagedPath(entry, fieldName);
  }
  const seen = new Set<string>();
  const duplicates = new Set<string>();
  for (const entry of entries) {
    if (seen.has(entry)) {
      duplicates.add(entry);
    }
    seen.add(entry);
  }
  if (duplicates.size > 0) {
    throw new Error(
      `.dotfiles.json: "${fieldName}" contains duplicate entry(ies): ${[...duplicates].join(", ")}`,
    );
  }
  for (let index = 1; index < entries.length; index++) {
    const previous = entries[index - 1]!;
    const current = entries[index]!;
    if (current < previous) {
      throw new Error(
        `.dotfiles.json: "${fieldName}" must list entries alphabetically; "${current}" should come before "${previous}"`,
      );
    }
  }
  return entries;
}

function validateManagedPath(relativePath: string, fieldName: string): void {
  if (relativePath === "" || relativePath === "." || path.isAbsolute(relativePath)) {
    throw new Error(`.dotfiles.json: "${fieldName}" contains invalid path "${relativePath}"`);
  }
  if (relativePath.split(/[\\/]/).includes("..")) {
    throw new Error(`.dotfiles.json: "${fieldName}" contains invalid path "${relativePath}"`);
  }
}

function validateNoConflicts(
  symlinkPaths: ReadonlySet<string>,
  copyPaths: ReadonlySet<string>,
): void {
  for (const symlinkPath of symlinkPaths) {
    for (const copyPath of copyPaths) {
      if (symlinkPath === copyPath) {
        throw new Error(
          `.dotfiles.json: "${symlinkPath}" appears in both symlink and copy`,
        );
      }
      if (isDescendant(symlinkPath, copyPath) || isDescendant(copyPath, symlinkPath)) {
        throw new Error(
          `.dotfiles.json: conflicting entries "${symlinkPath}" (symlink) and "${copyPath}" (copy) overlap`,
        );
      }
    }
  }
}

function isDescendant(ancestor: string, descendant: string): boolean {
  const relativePath = path.relative(ancestor, descendant);
  return relativePath !== "" && !relativePath.startsWith("..") && !path.isAbsolute(relativePath);
}

async function validatePrunePaths(
  sourceRoot: string,
  copyPaths: ReadonlySet<string>,
  prunePaths: ReadonlySet<string>,
): Promise<void> {
  for (const prunePath of prunePaths) {
    const isCoveredByCopyPath = [...copyPaths].some(
      (copyPath) => prunePath === copyPath || isDescendant(copyPath, prunePath),
    );
    if (isCoveredByCopyPath || !(await lstatOrNull(path.join(sourceRoot, prunePath)))) {
      continue;
    }
    throw new Error(`.dotfiles.json: prune path "${prunePath}" must be covered by copy paths`);
  }
}

async function validatePathsExist(
  sourceRoot: string,
  paths: ReadonlySet<string>,
  fieldName: string,
): Promise<void> {
  for (const relativePath of paths) {
    const targetPath = path.join(sourceRoot, relativePath);
    if (!(await lstatOrNull(targetPath))) {
      throw new Error(
        `.dotfiles.json: "${relativePath}" in ${fieldName} does not exist under ${sourceRoot}`,
      );
    }
  }
}
