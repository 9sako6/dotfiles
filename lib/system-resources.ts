import { mkdir, mkdtemp, readFile, readlink, realpath, rename, rm, symlink, writeFile } from "node:fs/promises";
import path from "node:path";
import { inspect, safeParents } from "./home-copy.ts";

export async function nightShift(repository: string) {
  const parse = async (file: string) => {
    try { return Bun.TOML.parse(await readFile(file, "utf8")); }
    catch { throw new Error("invalid dotfiles configuration"); }
  };
  const load = async (file: string) => await parse(file) as {
    services?: { agents?: unknown[] };
    settings?: { night_shift?: { start?: unknown; end?: unknown; temperature?: unknown } };
  };
  const shared = await load(path.join(repository, "dotfiles.toml"));
  const localPath = path.join(repository, "dotfiles.local.toml");
  const local = await inspect(localPath) ? await load(localPath) : {};
  if (local.services?.agents?.length || shared.services?.agents?.length) throw new Error("declare user agents in the private Nix module");
  const settings = { ...shared.settings?.night_shift, ...local.settings?.night_shift };
  if (!Object.keys(settings).length) return undefined;
  const { start, end, temperature } = settings;
  if (typeof start !== "string" || typeof end !== "string" ||
      ![start, end].every(time => /^([01]\d|2[0-3]):[0-5]\d$/.test(time)) ||
      typeof temperature !== "number" || !Number.isInteger(temperature) || temperature < 0 || temperature > 100) {
    throw new Error("invalid Night Shift settings");
  }
  return { start, end, temperature };
}

export async function deployResources(home: string, state: string, resources: string) {
  const ledger = path.join(state, "artifacts.json");
  await safeParents(path.dirname(state), ledger);
  const entry = await inspect(ledger);
  if (entry && !entry.isFile()) throw new Error("artifact record must be a regular file");
  const previous = entry ? JSON.parse(await readFile(ledger, "utf8")) : { version: 1, home, links: {} };
  if (previous.version !== 1 || previous.home !== home || !previous.links ||
      typeof previous.links !== "object" || Array.isArray(previous.links) ||
      !Object.values(previous.links).every(value => typeof value === "string") ||
      Object.keys(previous.retiring_directories ?? {}).length) throw new Error("invalid artifact ownership record");
  const targets: [string, string][] = [
    [".local/bin/localllm", "bin/localllm"],
    [".local/bin/nightlight", "bin/nightlight"],
    ["Library/Application Support/Anki2/addons21/anki-connect", "share/anki-connect"],
  ];
  const plans = [];
  for (const [relative, resource] of targets) {
    const target = path.join(home, relative);
    await safeParents(home, target);
    const source = path.join(resources, resource);
    const desired = await inspect(source) ? source : undefined;
    const existing = await inspect(target);
    if (!desired && !previous.links[relative]) continue;
    if (existing && !existing.isSymbolicLink()) throw new Error(`resource conflicts with existing file: ${target}`);
    const link = existing ? await readlink(target) : undefined;
    const samePackage = link && desired && (await realpath(target).catch(() => undefined)) === await realpath(source);
    if (link && link !== previous.links[relative] && link !== desired && !samePackage) {
      throw new Error(`resource conflicts with foreign link: ${target}`);
    }
    plans.push({ relative, target, desired, link });
  }
  for (const { relative, target, desired, link } of plans) {
    await mkdir(path.dirname(target), { recursive: true });
    const temporary = await mkdtemp(path.join(path.dirname(target), ".dotfiles-resource-"));
    try {
      if (desired && desired !== link) {
        await symlink(desired, path.join(temporary, "link"));
        await rename(path.join(temporary, "link"), target);
      } else if (!desired && link) await rm(target);
      if (desired) previous.links[relative] = desired;
      else delete previous.links[relative];
      const record = await mkdtemp(path.join(state, ".dotfiles-record-"));
      try {
        await writeFile(path.join(record, "file"), JSON.stringify(previous));
        await rename(path.join(record, "file"), ledger);
      } finally { await rm(record, { recursive: true, force: true }); }
    } finally { await rm(temporary, { recursive: true, force: true }); }
  }
}
