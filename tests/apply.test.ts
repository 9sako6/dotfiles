import { expect, test } from "bun:test";
import { chmod, cp, lstat, mkdir, readFile, readlink, realpath, rm, symlink, writeFile } from "node:fs/promises";
import path from "node:path";
import { applyCopies } from "../lib/home-copy.ts";
import { deployResources, nightShift, retireNightlightLink } from "../lib/system-resources.ts";
import { withTempDir, writeTree } from "./test-helpers.ts";

const repository = path.resolve(import.meta.dir, "..");

async function fixture(root: string) {
  const repo = path.join(root, "repo");
  const home = path.join(root, "home");
  await mkdir(home);
  await writeTree(repo, {
    "dotfiles.toml": 'copy = [".agents/skills", ".gitconfig"]\n',
    "home/.agents/skills/current/SKILL.md": "current",
    "home/.gitconfig": "configuration",
  });
  return { repo: await realpath(repo), home: await realpath(home) };
}

test("copy propagates edits and deletions within owned directories and preserves siblings and no-op files", async () => {
  await withTempDir("copy", async root => {
    const { repo, home } = await fixture(root);
    await writeTree(home, { ".agents/runtime": "keep", ".agents/skills/retired/SKILL.md": "remove" });
    await applyCopies(repo, home, "agents");
    const file = path.join(home, ".agents/skills/current/SKILL.md");
    const before = await lstat(file);
    await applyCopies(repo, home, "agents");
    const after = await lstat(file);
    expect([after.ino, after.mtimeMs]).toEqual([before.ino, before.mtimeMs]);
    expect(await Bun.file(path.join(home, ".agents/skills/retired/SKILL.md")).exists()).toBe(false);
    expect(await readFile(path.join(home, ".agents/runtime"), "utf8")).toBe("keep");
    expect(await Bun.file(path.join(home, ".gitconfig")).exists()).toBe(false);
    await writeFile(path.join(repo, "home/.agents/skills/current/SKILL.md"), "edited");
    await chmod(path.join(repo, "home/.agents/skills/current/SKILL.md"), 0o755);
    await applyCopies(repo, home, "agents");
    expect(await readFile(file, "utf8")).toBe("edited");
    expect((await lstat(file)).mode & 0o777).toBe(0o755);
    await rm(path.join(repo, "home/.agents/skills/current"), { recursive: true });
    await applyCopies(repo, home, "agents");
    expect(await Bun.file(file).exists()).toBe(false);
    await applyCopies(repo, home, "home");
    expect((await lstat(path.join(home, ".gitconfig"))).isFile()).toBe(true);
  });
});

for (const conflict of ["parent", "source", "target"] as const) {
  test(`copy rejects a ${conflict} symlink without writing outside HOME`, async () => {
    await withTempDir("copy-boundary", async root => {
      const { repo, home } = await fixture(root);
      const outside = path.join(root, "outside");
      await writeTree(outside, { "keep": "keep" });
      if (conflict === "parent") await symlink(outside, path.join(home, ".agents"));
      if (conflict === "source") await symlink(outside, path.join(repo, "home/.agents/skills/escape"));
      if (conflict === "target") {
        await mkdir(path.join(home, ".agents"));
        await symlink(path.join(repo, "home/.agents/skills"), path.join(home, ".agents/skills"));
      }
      await expect(applyCopies(repo, home, "agents")).rejects.toThrow();
      expect(await readFile(path.join(outside, "keep"), "utf8")).toBe("keep");
      expect(await Bun.file(path.join(outside, "current/SKILL.md")).exists()).toBe(false);
    });
  });
}

test("resources update recorded links, preserve Anki data and reject foreign links", async () => {
  await withTempDir("resources", async root => {
    const { home } = await fixture(root);
    const state = path.join(home, ".local/state/dotfiles");
    const resources = path.join(root, "resources");
    await mkdir(state, { recursive: true });
    await writeTree(resources, { "share/anki-connect/__init__.py": "addon" });
    await writeTree(home, { "Library/Application Support/Anki2/profile/collection.anki2": "collection" });
    await deployResources(home, state, resources);
    const next = path.join(root, "next");
    await writeTree(next, { "share/anki-connect/__init__.py": "next" });
    await deployResources(home, state, next);
    expect(await readFile(path.join(home, "Library/Application Support/Anki2/profile/collection.anki2"), "utf8")).toBe("collection");
    const anki = path.join(home, "Library/Application Support/Anki2/addons21/anki-connect");
    await rm(anki, { recursive: true, force: true });
    const foreign = path.join(root, "foreign");
    await symlink(path.join(next, "share/anki-connect"), foreign);
    await symlink(foreign, anki);
    await expect(deployResources(home, state, next)).rejects.toThrow("foreign link");
  });
});

for (const owned of [true, false]) {
  test(`Nightlight migration ${owned ? "removes the recorded" : "preserves a replacement"} link`, async () => {
    await withTempDir("nightlight-migration", async root => {
      const { home } = await fixture(root);
      const state = path.join(home, ".local/state/dotfiles");
      const target = path.join(home, ".local/bin/nightlight");
      const old = path.join(root, "old/nightlight");
      const replacement = path.join(root, "custom/nightlight");
      const links = { ".local/bin/nightlight": old, "addon": "preserve" };
      await writeTree(state, { "artifacts.json": JSON.stringify({ version: 1, home, links }) });
      await mkdir(path.dirname(target), { recursive: true });
      await symlink(owned ? old : replacement, target);
      await retireNightlightLink(home, state);
      if (owned) await expect(lstat(target)).rejects.toMatchObject({ code: "ENOENT" });
      else expect(await readlink(target)).toBe(replacement);
      const record = JSON.parse(await readFile(path.join(state, "artifacts.json"), "utf8"));
      expect(record.links).toEqual({ addon: "preserve" });
    });
  });
}

test("Night Shift merges local fields and rejects invalid input without exposing it", async () => {
  await withTempDir("settings", async root => {
    await writeTree(root, { "dotfiles.toml": '[settings.night_shift]\nstart = "22:00"\nend = "07:00"\ntemperature = 80\n', "dotfiles.local.toml": '[settings.night_shift]\ntemperature = 60\n' });
    expect(await nightShift(root)).toEqual({ start: "22:00", end: "07:00", temperature: 60 });
    await writeFile(path.join(root, "dotfiles.local.toml"), '[settings.night_shift]\nstart = "private-invalid-value"\n');
    await expect(nightShift(root)).rejects.toThrow("invalid Night Shift settings");
  });
});

test("agent generation preserves deployed resources on failure and can be retried", async () => {
  await withTempDir("agents-apply", async root => {
    const { repo, home } = await fixture(root);
    for (const directory of ["bin", "lib"]) {
      await mkdir(path.join(repo, directory), { recursive: true });
    }
    for (const file of ["bin/agents-apply.ts", "lib/home-copy.ts", "lib/process.ts"]) {
      await cp(path.join(repository, file), path.join(repo, file));
    }
    const bin = path.join(root, "bin");
    await writeTree(bin, {
      "apm": '#!/bin/sh\nif [ "$1" = compile ]; then\n  [ "${FAIL_COMPILE:-}" != 1 ] || exit 1\n  printf generated > .agents/skills/current/SKILL.md\nfi\n',
    });
    await chmod(path.join(bin, "apm"), 0o755);
    const execute = async (extra: NodeJS.ProcessEnv = {}) => {
      const child = Bun.spawn([process.execPath, path.join(repo, "bin/agents-apply.ts")], {
        env: { ...process.env, HOME: home, PATH: `${bin}:/usr/bin:/bin`, ...extra }, stdout: "pipe", stderr: "pipe",
      });
      const [code, output] = await Promise.all([child.exited, new Response(child.stderr).text()]);
      return { code, output };
    };
    expect(await execute()).toMatchObject({ code: 0 });
    expect(await readFile(path.join(home, ".agents/skills/current/SKILL.md"), "utf8")).toBe("generated");
    await writeFile(path.join(repo, "home/.agents/skills/current/SKILL.md"), "pending");
    expect((await execute({ FAIL_COMPILE: "1" })).code).not.toBe(0);
    expect(await readFile(path.join(home, ".agents/skills/current/SKILL.md"), "utf8")).toBe("generated");
    expect(await execute()).toMatchObject({ code: 0 });
    expect(await readFile(path.join(home, ".agents/skills/current/SKILL.md"), "utf8")).toBe("generated");
  });
});
