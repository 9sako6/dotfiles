import { basename } from "../deps.ts";
import { exist } from "./exist.ts";

export class NameMismatchError extends Error { }

export const setSymlink = async (srcPath: string, destPath: string) => {
  // Prevent to overwrite a wrong file.
  if (basename(srcPath) !== basename(destPath)) {
    console.log(srcPath, destPath)
    throw new NameMismatchError(
      "the source file/dir name is different from the destination file/dir name.",
    );
  }
  const { isDirectory, isFile } = await Deno.lstat(srcPath);
  if (isFile) {
    if (await exist(destPath)) await Deno.remove(destPath);

    await Deno.symlink(srcPath, destPath);
  } else if (isDirectory) {
    if (!(await exist(destPath))) {
      await Deno.mkdir(destPath);
    }

    const sources: Deno.DirEntry[] = [];
    for await (const fileOrDir of Deno.readDir(srcPath)) {
      sources.push(fileOrDir);
    }

    await Promise.all(
      sources.map((source) =>
        setSymlink(
          `${srcPath}/${source.name}`,
          `${destPath}/${source.name}`,
        )
      ),
    );
  }
};
