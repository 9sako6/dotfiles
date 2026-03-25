import { mkdtemp, mkdir, readlink, realpath, rm, stat, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

export async function withTempDir<T>(name: string, run: (dir: string) => Promise<T>): Promise<T> {
  const dir = await mkdtemp(path.join(tmpdir(), `${name}-`));
  try {
    return await run(dir);
  } finally {
    await rm(dir, { force: true, recursive: true });
  }
}

export async function writeTree(root: string, files: Record<string, string>) {
  for (const [relativePath, content] of Object.entries(files)) {
    const filePath = path.join(root, relativePath);
    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(filePath, content);
  }
}

export async function ensureDir(root: string, relativePath: string) {
  await mkdir(path.join(root, relativePath), { recursive: true });
}

export async function readSymlinkTarget(linkPath: string) {
  return realpath(path.resolve(path.dirname(linkPath), await readlink(linkPath)));
}

export async function isSymlink(targetPath: string) {
  return (await stat(targetPath, { throwIfNoEntry: false }))?.isSymbolicLink?.() ?? false;
}

export async function createSymlink(targetPath: string, linkPath: string) {
  await mkdir(path.dirname(linkPath), { recursive: true });
  await symlink(targetPath, linkPath);
}
