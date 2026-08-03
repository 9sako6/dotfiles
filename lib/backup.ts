import { lstat, mkdir } from "node:fs/promises";
import path from "node:path";

function pad(value: number) {
  return String(value).padStart(2, "0");
}

export function createTimestamp(date = new Date()) {
  return [
    date.getFullYear(),
    pad(date.getMonth() + 1),
    pad(date.getDate()),
    "T",
    pad(date.getHours()),
    pad(date.getMinutes()),
    pad(date.getSeconds()),
  ].join("");
}

export function backupRootFor(homeDir: string, timestamp: string) {
  return path.join(homeDir, ".dotfiles-backups", timestamp);
}

export async function allocateBackupRoot(
  homeDir: string,
  timestamp: string,
  reserve: boolean,
): Promise<string> {
  const backupsRoot = path.join(homeDir, ".dotfiles-backups");
  if (reserve) {
    await mkdir(backupsRoot, { recursive: true });
  }

  for (let generation = 1; ; generation += 1) {
    const name = generation === 1 ? timestamp : `${timestamp}-${generation}`;
    const candidate = path.join(backupsRoot, name);
    try {
      if (reserve) {
        await mkdir(candidate);
        return candidate;
      }
      await lstat(candidate);
    } catch (error) {
      if (isFileSystemError(error, reserve ? "EEXIST" : "ENOENT")) {
        if (reserve) {
          continue;
        }
        return candidate;
      }
      throw error;
    }
  }
}

export function backupPathFor(homeDir: string, destinationPath: string, timestamp: string) {
  return path.join(backupRootFor(homeDir, timestamp), path.relative(homeDir, destinationPath));
}

function isFileSystemError(error: unknown, code: string): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error && error.code === code;
}
