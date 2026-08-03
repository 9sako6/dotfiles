import { lstat, mkdir, readdir, rmdir } from "node:fs/promises";
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

export async function allocateBackupRoot(homeDir: string, timestamp: string): Promise<string> {
  const backupsRoot = path.join(homeDir, ".dotfiles-backups");
  for (let generation = 1; ; generation += 1) {
    const name = generation === 1 ? timestamp : `${timestamp}-${generation}`;
    const candidate = path.join(backupsRoot, name);
    try {
      await lstat(candidate);
    } catch (error) {
      if (isFileSystemError(error, "ENOENT")) {
        return candidate;
      }
      throw error;
    }
  }
}

export async function reserveBackupRoot(backupRoot: string): Promise<void> {
  await mkdir(path.dirname(backupRoot), { recursive: true });
  try {
    await mkdir(backupRoot);
  } catch (error) {
    if (isFileSystemError(error, "EEXIST")) {
      throw new Error(`backup root already exists: ${backupRoot}`);
    }
    throw error;
  }
}

export async function removeEmptyBackupRoot(backupRoot: string): Promise<void> {
  let entries;
  try {
    entries = await readdir(backupRoot, { withFileTypes: true });
  } catch {
    return;
  }
  for (const entry of entries) {
    if (entry.isDirectory()) {
      await removeEmptyBackupRoot(path.join(backupRoot, entry.name));
    }
  }
  await rmdir(backupRoot).catch(() => {});
}

export function backupPathFor(homeDir: string, destinationPath: string, timestamp: string) {
  return path.join(backupRootFor(homeDir, timestamp), path.relative(homeDir, destinationPath));
}

function isFileSystemError(error: unknown, code: string): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error && error.code === code;
}
