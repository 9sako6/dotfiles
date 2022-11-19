import { basename } from "../deps.ts";
import { exist } from "./exist.ts";

export class NameMismatchError extends Error {}

export const setSymlink = async (srcPath: string, destPath: string) => {
  // Prevent to overwrite a wrong file.
  if (basename(srcPath) !== basename(destPath)) {
    throw new NameMismatchError(
      "the source file/dir name is different from the destination file/dir name.",
    );
  }
  if (await exist(destPath)) {
    await Deno.remove(destPath, { recursive: true });
  }
  await Deno.symlink(srcPath, destPath);
};
