import { lstat, readdir, realpath } from "node:fs/promises";

export async function lstatOrNull(targetPath: string) {
  try {
    return await lstat(targetPath);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return null;
    }
    throw error;
  }
}

export async function realpathOrNull(targetPath: string) {
  try {
    return await realpath(targetPath);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return null;
    }
    throw error;
  }
}

export async function readDirents(targetPath: string) {
  return readdir(targetPath, { withFileTypes: true });
}
