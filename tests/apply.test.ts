import { expect, test } from "bun:test";
import { chmod, cp, lstat, mkdir, readFile, readlink, realpath, rm, symlink, writeFile } from "node:fs/promises";
import path from "node:path";
import { applyCopies } from "../lib/home-copy.ts";
import { deployResources, nightShift } from "../lib/system-resources.ts";
import { withTempDir, writeTree } from "./test-helpers.ts";

const repository = path.resolve(import.meta.dir, "..");
const mise = Bun.which("mise");

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
    await writeTree(resources, { "bin/nightlight": "night", "share/anki-connect/__init__.py": "addon" });
    await writeTree(home, { "Library/Application Support/Anki2/profile/collection.anki2": "collection" });
    await deployResources(home, state, resources);
    expect(await readlink(path.join(home, ".local/bin/nightlight"))).toBe(path.join(resources, "bin/nightlight"));
    const next = path.join(root, "next");
    await writeTree(next, { "bin/nightlight": "next", "share/anki-connect/__init__.py": "next" });
    await deployResources(home, state, next);
    expect(await readlink(path.join(home, ".local/bin/nightlight"))).toBe(path.join(next, "bin/nightlight"));
    expect(await readFile(path.join(home, "Library/Application Support/Anki2/profile/collection.anki2"), "utf8")).toBe("collection");
    const anki = path.join(home, "Library/Application Support/Anki2/addons21/anki-connect");
    await rm(anki, { recursive: true, force: true });
    const foreign = path.join(root, "foreign");
    await symlink(path.join(next, "share/anki-connect"), foreign);
    await symlink(foreign, anki);
    await expect(deployResources(home, state, next)).rejects.toThrow("foreign link");
  });
});

test("Night Shift merges local fields and rejects invalid input without exposing it", async () => {
  await withTempDir("settings", async root => {
    await writeTree(root, { "dotfiles.toml": '[settings.night_shift]\nstart = "22:00"\nend = "07:00"\ntemperature = 80\n', "dotfiles.local.toml": '[settings.night_shift]\ntemperature = 60\n' });
    expect(await nightShift(root)).toEqual({ start: "22:00", end: "07:00", temperature: 60 });
    await writeFile(path.join(root, "dotfiles.local.toml"), '[settings.night_shift]\nstart = "private-invalid-value"\n');
    await expect(nightShift(root)).rejects.toThrow("invalid Night Shift settings");
  });
});

test.skipIf(!mise)("actual mise tasks deploy home and agents independently without Nix or Rust", async () => {
  await withTempDir("public-apply", async root => {
    const { repo, home } = await fixture(root);
    for (const directory of ["bin", "lib"]) {
      await mkdir(path.join(repo, directory), { recursive: true });
    }
    for (const file of [".mise.toml", "bin/home-apply.ts", "bin/agents-apply.ts", "lib/home-copy.ts", "lib/process.ts"]) {
      await cp(path.join(repository, file), path.join(repo, file));
    }
    const bin = path.join(root, "bin");
    const log = path.join(root, "commands");
    await writeTree(repo, {
      "home/.config/mise/config.toml": "[settings]\nauto_install = false\n",
      "home/.gitignore_global": "ignore", "home/.zshenv": "environment", "home/.zshrc": "shell",
      "home/mybin/fixture": "executable", "home/.zsh.d/secrets.zsh": "untracked secret",
      ...Object.fromEntries(["alias", "functions", "keybindings", "prompt", "ssh-agent"].map(name => [`home/.zsh.d/${name}.zsh`, name])),
    });
    await writeTree(bin, {
      "mise": '#!/bin/sh\ncase "$1" in\ndotfiles) exec "$REAL_MISE" "$@" ;;\ninstall|bootstrap) printf "%s:%s\\n" "$*" "$MISE_CONFIG_FILE" >> "$APPLY_LOG" ;;\n*) exit 99 ;;\nesac\n',
      "apm": '#!/bin/sh\nprintf "apm:%s:%s\\n" "$*" "$PWD" >> "$APPLY_LOG"\nif [ "$1" = compile ]; then\n  [ "${FAIL_COMPILE:-}" != 1 ] || exit 1\n  printf generated > .agents/skills/current/SKILL.md\nfi\n',
      ...Object.fromEntries(["nix", "sudo", "cargo", "rustc"].map(name => [name, "#!/bin/sh\nexit 99\n"])),
    });
    for (const command of ["mise", "apm", "nix", "sudo", "cargo", "rustc"]) await chmod(path.join(bin, command), 0o755);
    const env = { ...process.env, HOME: home, PATH: `${bin}:${path.dirname(process.execPath)}:/usr/bin:/bin`, REAL_MISE: mise!, APPLY_LOG: log, MISE_TRUSTED_CONFIG_PATHS: repo, MISE_GLOBAL_CONFIG_FILE: path.join(root, "global.toml"), MISE_CONFIG_FILE: "", MISE_DATA_DIR: path.join(root, "mise-data"), MISE_CACHE_DIR: path.join(root, "mise-cache") };
    const execute = async (task: string, extra: NodeJS.ProcessEnv = {}) => {
      const child = Bun.spawn([mise!, "--cd", repo, "run", task], { env: { ...env, ...extra }, stdout: "pipe", stderr: "pipe" });
      const [code, output] = await Promise.all([child.exited, new Response(child.stderr).text()]);
      return { code, output };
    };
    expect(await execute("home:apply")).toMatchObject({ code: 0 });
    expect(await execute("home:apply")).toMatchObject({ code: 0 });
    expect(await realpath(path.join(home, ".config/mise/config.toml"))).toBe(await realpath(path.join(repo, "home/.config/mise/config.toml")));
    expect(await readFile(path.join(home, ".gitconfig"), "utf8")).toBe("configuration");
    expect(await Bun.file(path.join(home, ".zsh.d/secrets.zsh")).exists()).toBe(false);
    expect(await Bun.file(path.join(home, ".agents/skills/current/SKILL.md")).exists()).toBe(false);
    expect(await execute("agents:apply")).toMatchObject({ code: 0 });
    expect(await readFile(path.join(home, ".agents/skills/current/SKILL.md"), "utf8")).toBe("generated");
    await writeFile(path.join(repo, "home/.agents/skills/current/SKILL.md"), "pending");
    expect((await execute("agents:apply", { FAIL_COMPILE: "1" })).code).not.toBe(0);
    expect(await readFile(path.join(home, ".agents/skills/current/SKILL.md"), "utf8")).toBe("generated");
    expect(await execute("agents:apply")).toMatchObject({ code: 0 });
    const commands = await readFile(log, "utf8");
    expect(commands).toContain(`install --locked:${repo}/home/.config/mise/config.toml`);
    expect(commands).toContain(`bootstrap repos apply --yes:${repo}/home/.config/mise/config.toml`);
    expect(commands).toContain(`apm:install --frozen --only apm:${repo}/home`);
    expect(commands).toContain(`apm:compile --clean:${repo}/home`);
  });
}, 30000);
