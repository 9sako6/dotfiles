import { expect, test } from "bun:test";
import { chmod, cp, mkdir, readFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers.ts";

const repository = path.resolve(import.meta.dir, "..");

test.skipIf(process.platform !== "darwin" || process.arch !== "arm64" || process.getuid?.() === 0)("system:apply limits build concurrency and stops before deployment when the build fails", async () => {
  await withTempDir("system-apply", async root => {
    const repo = path.join(root, "repo");
    const home = path.join(root, "home");
    const state = path.join(home, ".local/state");
    const nix = path.join(root, "bin/nix");
    const log = path.join(root, "build.json");
    await mkdir(home);
    await writeTree(repo, {
      "dotfiles.toml": "copy = []\n",
      "lib/install-system.sh": 'install_system_ensure_lix() { printf "%s\\n" "$DOTFILES_TEST_NIX"; }\n',
    });
    for (const relative of ["bin/system-apply.ts", "lib/home-copy.ts", "lib/process.ts", "lib/system-resources.ts"]) {
      await mkdir(path.dirname(path.join(repo, relative)), { recursive: true });
      await cp(path.join(repository, relative), path.join(repo, relative));
    }
    await writeTree(root, {
      "bin/nix": `#!${process.execPath}\nawait Bun.write(process.env.DOTFILES_TEST_BUILD_LOG!, JSON.stringify(process.argv.slice(2)));\nprocess.exit(42);\n`,
      "home/.local/bin/nightlight": "existing helper",
    });
    await chmod(nix, 0o755);
    const child = Bun.spawn([process.execPath, path.join(repo, "bin/system-apply.ts")], {
      cwd: repo,
      env: {
        ...process.env,
        HOME: home,
        XDG_STATE_HOME: state,
        DOTFILES_TEST_NIX: nix,
        DOTFILES_TEST_BUILD_LOG: log,
      },
      stdout: "pipe",
      stderr: "pipe",
    });
    const [code, , stderr] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
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
