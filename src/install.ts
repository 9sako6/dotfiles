import { setSymlink } from "./symlink.ts";

export const install = async (sourceDir: string, destDir: string) => {
  const sources: Deno.DirEntry[] = [];
  for await (const fileOrDir of Deno.readDir(sourceDir)) {
    sources.push(fileOrDir);
  }
  const absoluteSourceDir = await Deno.realPath(sourceDir);
  const absoluteDestDir = await Deno.realPath(destDir);

  await Promise.all(
    sources.map((source) =>
      setSymlink(
        `${absoluteSourceDir}/${source.name}`,
        `${absoluteDestDir}/${source.name}`,
      )
    ),
  );
};
