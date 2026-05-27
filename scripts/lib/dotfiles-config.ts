import { readFile } from "node:fs/promises";
import path from "node:path";
import { lstatOrNull } from "./fs";

export type DotfilesConfig = {
  symlinkPaths: ReadonlySet<string>;
  copyPaths: ReadonlySet<string>;
  prunePaths: ReadonlySet<string>;
};

export async function loadDotfilesConfig(
  repoRoot: string,
  sourceRoot: string,
): Promise<DotfilesConfig> {
  const configPath = path.join(repoRoot, ".dotfiles.json");
  const raw = await readFile(configPath, "utf8");
  const parsed = JSON.parse(raw) as { symlink?: unknown; copy?: unknown; prune?: unknown };
  const symlinkPaths = new Set(parseManagedPaths(parsed.symlink, "symlink"));
  const copyPaths = new Set(parseManagedPaths(parsed.copy, "copy"));
  const prunePaths = new Set(parseManagedPaths(parsed.prune, "prune"));
  validateNoConflicts(symlinkPaths, copyPaths);
  validatePrunePaths(copyPaths, prunePaths);
  await validatePathsExist(sourceRoot, symlinkPaths, "symlink");
  await validatePathsExist(sourceRoot, copyPaths, "copy");
  await validatePathsExist(sourceRoot, prunePaths, "prune");
  return { symlinkPaths, copyPaths, prunePaths };
}

function parseManagedPaths(value: unknown, fieldName: string): string[] {
  if (value === undefined) {
    return [];
  }
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== "string")) {
    throw new Error(`.dotfiles.json: "${fieldName}" must be an array of strings`);
  }
  for (const entry of value) {
    validateManagedPath(entry, fieldName);
  }
  return value;
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

function validatePrunePaths(
  copyPaths: ReadonlySet<string>,
  prunePaths: ReadonlySet<string>,
): void {
  for (const prunePath of prunePaths) {
    const isCoveredByCopyPath = [...copyPaths].some(
      (copyPath) => prunePath === copyPath || isDescendant(copyPath, prunePath),
    );
    if (!isCoveredByCopyPath) {
      throw new Error(`.dotfiles.json: prune path "${prunePath}" must be covered by copy paths`);
    }
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
