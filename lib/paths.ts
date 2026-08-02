import path from "node:path";

export function sourceToDestinationPath(sourceRoot: string, sourcePath: string, homeDir: string) {
  return path.join(homeDir, path.relative(sourceRoot, sourcePath));
}
