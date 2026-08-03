import { realpath } from "node:fs/promises";
import path from "node:path";

export async function resolveRepoRoot(entrypointPath: string): Promise<string> {
  return path.dirname(path.dirname(await realpath(entrypointPath)));
}

export function managedHomeRoot(repoRoot: string): string {
  return path.join(repoRoot, "home");
}

export function sourceToDestinationPath(sourceRoot: string, sourcePath: string, homeDir: string) {
  return path.join(homeDir, path.relative(sourceRoot, sourcePath));
}
