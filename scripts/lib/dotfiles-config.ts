import { readFile } from "node:fs/promises";
import path from "node:path";
import { lstatOrNull } from "./fs";

export type DotfilesConfig = {
  symlinkPaths: ReadonlySet<string>;
  copyPaths: ReadonlySet<string>;
};

export async function loadDotfilesConfig(
  repoRoot: string,
  sourceRoot: string,
): Promise<DotfilesConfig> {
  const configPath = path.join(repoRoot, ".dotfiles.json");
  const raw = await readFile(configPath, "utf8");
  const parsed = JSON.parse(raw) as { symlink?: unknown; copy?: unknown };
  const symlinkPaths = new Set(parseStringArray(parsed.symlink, "symlink"));
  const copyPaths = new Set(parseStringArray(parsed.copy, "copy"));
  validateNoConflicts(symlinkPaths, copyPaths);
  await validatePathsExist(sourceRoot, symlinkPaths, "symlink");
  await validatePathsExist(sourceRoot, copyPaths, "copy");
  return { symlinkPaths, copyPaths };
}

function parseStringArray(value: unknown, fieldName: string): string[] {
  if (value === undefined) {
    return [];
  }
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== "string")) {
    throw new Error(`.dotfiles.json: "${fieldName}" must be an array of strings`);
  }
  return value;
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
