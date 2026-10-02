import { chmod, lstat, mkdir, mkdtemp, readFile, readdir, readlink, rename, rm, writeFile } from "node:fs/promises";
import path from "node:path";

export async function inspect(file: string) {
  try { return await lstat(file); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
    throw error;
  }
}

export async function safeParents(home: string, target: string) {
  const relative = path.relative(home, target);
  if (!relative || relative.startsWith("../") || path.isAbsolute(relative)) throw new Error("target must be inside HOME");
  let parent = home;
  for (const part of relative.split(path.sep).slice(0, -1)) {
    parent = path.join(parent, part);
    const entry = await inspect(parent);
    if (entry && !entry.isDirectory()) throw new Error(`refusing non-directory parent: ${parent}`);
  }
}

async function validateSource(source: string) {
  const entry = await lstat(source);
  if (entry.isDirectory()) {
    for (const name of await readdir(source)) await validateSource(path.join(source, name));
  } else if (!entry.isFile()) throw new Error(`copy source must contain only regular files: ${source}`);
}

async function sync(source: string, target: string) {
  const sourceEntry = await lstat(source);
  let targetEntry = await inspect(target);
  if (targetEntry?.isSymbolicLink()) {
    const link = path.resolve(path.dirname(target), await readlink(target));
    if (link !== source) throw new Error(`refusing copy over symlink: ${target}`);
    await rm(target);
    targetEntry = undefined;
  }
  if (sourceEntry.isDirectory()) {
    if (targetEntry && !targetEntry.isDirectory()) throw new Error(`copy directory conflicts with file: ${target}`);
    await mkdir(target, { recursive: true });
    const names = await readdir(source);
    for (const name of names) await sync(path.join(source, name), path.join(target, name));
    for (const name of await readdir(target)) {
      if (!names.includes(name)) await rm(path.join(target, name), { recursive: true, force: true });
    }
    return;
  }
  if (targetEntry && !targetEntry.isFile()) throw new Error(`copy file conflicts with directory: ${target}`);
  const mode = (sourceEntry.mode & 0o777) | 0o200;
  const bytes = await readFile(source);
  if (targetEntry && (targetEntry.mode & 0o777) === mode && bytes.equals(await readFile(target))) return;
  await mkdir(path.dirname(target), { recursive: true });
  const temporary = await mkdtemp(path.join(path.dirname(target), ".dotfiles-copy-"));
  try {
    const file = path.join(temporary, "file");
    await writeFile(file, bytes);
    await chmod(file, mode);
    await rename(file, target);
  } finally { await rm(temporary, { recursive: true, force: true }); }
}

export async function applyCopies(repository: string, home: string, scope: "home" | "agents") {
  const configuration = Bun.TOML.parse(await readFile(path.join(repository, "dotfiles.toml"), "utf8")) as { copy?: unknown };
  const paths = configuration.copy;
  if (!Array.isArray(paths) || !paths.every(value => typeof value === "string")) throw new Error("copy must be a path list");
  if (JSON.stringify(paths) !== JSON.stringify([...new Set(paths)].sort())) throw new Error("copy paths must be unique and alphabetical");
  for (const relative of paths) {
    if (!relative || path.isAbsolute(relative) || relative.split("/").some(part => !part || part === "." || part === "..")) throw new Error("invalid copy path");
    if (paths.some(other => other !== relative && other.startsWith(`${relative}/`))) throw new Error("copy paths overlap");
  }
  const selected = paths.filter(relative => /^\.(agents|claude|codex)\//.test(relative) === (scope === "agents"));
  for (const relative of selected) {
    await safeParents(path.join(repository, "home"), path.join(repository, "home", relative));
    await validateSource(path.join(repository, "home", relative));
    await safeParents(home, path.join(home, relative));
  }
  for (const relative of selected) await sync(path.join(repository, "home", relative), path.join(home, relative));
}
