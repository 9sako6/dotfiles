import { expect, test } from "bun:test";
import { chmod, cp, lstat, mkdir, readFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers.ts";

const repository = path.resolve(import.meta.dir, "..");
const supported = process.platform === "darwin" && process.arch === "arm64" && process.getuid?.() !== 0;
const mise = Bun.which("mise");

async function fixture(root: string, buildExit: number) {
  const repo = path.join(root, "repo");
  const home = path.join(root, "home");
  const state = path.join(home, ".local/state");
  const nix = path.join(root, "bin/nix");
  const log = path.join(root, "build.json");
  const ensureLog = path.join(root, "ensure.log");
  await mkdir(home);
  await writeTree(repo, {
    "dotfiles.toml": "copy = []\n",
    "lib/install-system.sh": `install_system_ensure_lix() { printf 'ensure' > "$DOTFILES_TEST_ENSURE_LOG"; printf '%s\\n' "$DOTFILES_TEST_NIX"; }
install_system_resolve_lix() { [ "\${DOTFILES_TEST_LIX_MISSING:-0}" != 1 ] || return 1; printf '%s\\n' "$DOTFILES_TEST_NIX"; }
install_system_fail_lix_resolution() { printf 'trusted Lix installation was not found\\n' >&2; exit 1; }
`,
  });
  for (const relative of [".mise.toml", "bin/system-apply.ts", "lib/home-copy.ts", "lib/process.ts", "lib/system-resources.ts"]) {
    await mkdir(path.dirname(path.join(repo, relative)), { recursive: true });
    await cp(path.join(repository, relative), path.join(repo, relative));
  }
  await writeTree(root, {
    "bin/nix": `#!${process.execPath}\nawait Bun.write(process.env.DOTFILES_TEST_BUILD_LOG!, JSON.stringify(process.argv.slice(2)));\nprocess.exit(${buildExit});\n`,
    "home/.local/bin/nightlight": "existing helper",
  });
  await chmod(nix, 0o755);
  const env: NodeJS.ProcessEnv = {
    ...Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith("MISE_") && !name.startsWith("__MISE_"))),
    HOME: home,
    XDG_STATE_HOME: state,
    PATH: `${path.dirname(process.execPath)}:/usr/bin:/bin`,
    MISE_CONFIG_FILE: "",
    MISE_GLOBAL_CONFIG_FILE: path.join(root, "global.toml"),
    MISE_TRUSTED_CONFIG_PATHS: repo,
    MISE_DATA_DIR: path.join(root, "mise-data"),
    MISE_CACHE_DIR: path.join(root, "mise-cache"),
    MISE_STATE_DIR: path.join(root, "mise-state"),
    DOTFILES_TEST_NIX: nix,
    DOTFILES_TEST_BUILD_LOG: log,
    DOTFILES_TEST_ENSURE_LOG: ensureLog,
  };
  const execute = async (args: string[] = [], useMise = false, extra: NodeJS.ProcessEnv = {}) => {
    const child = Bun.spawn(useMise ? [mise!, "--cd", repo, "run", "system:check"] : [process.execPath, path.join(repo, "bin/system-apply.ts"), ...args], {
      cwd: repo,
      env: { ...env, ...extra },
      stdout: "pipe",
      stderr: "pipe",
    });
    const [code, , stderr] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
    return { code, stderr };
  };
  return { repo, home, state, nix, log, execute, ensureLog };
}

test.skipIf(!supported)("system:apply limits build concurrency and stops before deployment when the build fails", async () => {
  await withTempDir("system-apply", async root => {
    const { home, state, nix, log, execute } = await fixture(root, 42);
    const { code, stderr } = await execute();
    expect(code).not.toBe(0);
    expect(stderr).toContain(`${nix} failed`);
    const args = JSON.parse(await readFile(log, "utf8")) as string[];
    expect(args).toContain("build");
    expect(args[args.indexOf("--max-jobs") + 1]).toBe("1");
    expect(args[args.indexOf("--cores") + 1]).toBe("2");
    expect(await readFile(path.join(home, ".local/bin/nightlight"), "utf8")).toBe("existing helper");
    expect(await Bun.file(path.join(state, "dotfiles/artifacts.json")).exists()).toBe(false);
  });
});

test.skipIf(!supported || !mise)("system:check previews builds through the mise task without installation or deployment", async () => {
  await withTempDir("system-check", async root => {
    const { home, state, log, execute, ensureLog } = await fixture(root, 0);
    expect(await execute([], true)).toMatchObject({ code: 0 });
    const args = JSON.parse(await readFile(log, "utf8")) as string[];
    expect(args).toContain("--dry-run");
    expect(args).toContain("--no-link");
    expect(args).not.toContain("--out-link");
    expect(args.slice(args.indexOf("--option"), args.indexOf("--option") + 3)).toEqual(["--option", "allow-import-from-derivation", "false"]);
    expect(await Bun.file(ensureLog).exists()).toBe(false);
    await expect(lstat(path.join(state, "dotfiles"))).rejects.toMatchObject({ code: "ENOENT" });
    expect(await readFile(path.join(home, ".local/bin/nightlight"), "utf8")).toBe("existing helper");
  });
});

test.skipIf(!supported)("system:check reports missing Lix without starting installation or builds", async () => {
  await withTempDir("system-check-missing", async root => {
    const { log, execute, ensureLog } = await fixture(root, 0);
    const result = await execute(["--dry-run"], false, { DOTFILES_TEST_LIX_MISSING: "1" });
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("trusted Lix installation was not found");
    expect(await Bun.file(ensureLog).exists()).toBe(false);
    expect(await Bun.file(log).exists()).toBe(false);
  });
});

test.skipIf(!supported)("system:apply rejects unknown arguments before starting builds", async () => {
  await withTempDir("system-apply-arguments", async root => {
    const { log, execute, ensureLog } = await fixture(root, 0);
    const result = await execute(["--dryrun"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("usage:");
    expect(await Bun.file(ensureLog).exists()).toBe(false);
    expect(await Bun.file(log).exists()).toBe(false);
  });
});
