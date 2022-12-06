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
  const { isDirectory, isFile } = await Deno.lstat(srcPath);
  const destExists = await exist(destPath);
  if (isFile) {
    // if (destExists) console.log(await Deno.lstat(destPath))
    if (destExists && (await Deno.lstat(destPath)).isSymlink) return;
    if (destExists) await Deno.rename(destPath, `${destPath}.old`);

    await Deno.symlink(srcPath, destPath);
  } else if (isDirectory) {
    if (!destExists) {
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
