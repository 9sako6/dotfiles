import { describe, expect, test } from "bun:test";
import { lstat, mkdir, mkdtemp, readFile, realpath, rename, rm, symlink, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import path from "node:path";

const agentScript = path.resolve(import.meta.dir, "../home/.zsh.d/ssh-agent.zsh");

async function startedPids(root: string) {
  let output = "";
  try {
    output = await readFile(path.join(root, "started"), "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
  }
  return new Set(Array.from(output.matchAll(/SSH_AGENT_PID=(\d+)/g), (match) => Number(match[1])));
}

async function createStaleSocket(socket: string) {
  const listeningSocket = `${socket}.listening`;
  const server = createServer();
  try {
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(listeningSocket, resolve);
    });
    await rename(listeningSocket, socket);
  } finally {
    if (server.listening) {
      await new Promise<void>((resolve, reject) => {
        server.close((error) => error ? reject(error) : resolve());
      });
    }
  }
  expect((await lstat(socket)).isSocket()).toBe(true);
}

async function createFixture(root: string) {
  const home = path.join(root, "home");
  const socket = path.join(home, ".ssh/agent.sock");
  const binary = path.join(root, "bin");
  const realAgent = Bun.which("ssh-agent");
  if (!realAgent) throw new Error("SSH agent must be available in the contract-test environment");

  await mkdir(path.join(home, ".ssh"), { recursive: true, mode: 0o700 });
  await mkdir(binary);
  await writeFile(path.join(binary, "ssh-agent"), `#!/bin/sh
set -eu
output="$("$REAL_SSH_AGENT" "$@")"
printf '%s\\n' "$output" >> "$AGENT_STARTS"
printf '%s\\n' "$output"
`, { mode: 0o755 });

  const environment = {
    ...process.env,
    HOME: home,
    SSH_AUTH_SOCK: socket,
    PATH: `${binary}${path.delimiter}${process.env.PATH ?? ""}`,
    REAL_SSH_AGENT: realAgent,
    AGENT_STARTS: path.join(root, "started"),
  };

  async function command(args: string[], overrides: NodeJS.ProcessEnv = {}, timeout = 5_000) {
    const child = Bun.spawn(args, {
      env: { ...environment, ...overrides },
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
      timeout,
    });
    const [stdout, stderr, code] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    return { code, stdout, stderr };
  }

  function setupShell(overrides: NodeJS.ProcessEnv = {}) {
    return command(["zsh", "-f", "-c", '. "$1"; printf "%s" "${SSH_AUTH_SOCK:-}"',
      "ssh-fixture", agentScript], overrides, 15_000);
  }

  async function expectConnected() {
    const result = await command(["ssh-add", "-l"]);
    expect([0, 1], result.stderr).toContain(result.code);
    return result;
  }

  return { root, socket, command, setupShell, expectConnected, pids: () => startedPids(root) };
}

async function withAgentFixture(run: (fixture: Awaited<ReturnType<typeof createFixture>>) => Promise<void>) {
  const root = await realpath(await mkdtemp("/tmp/dot-ssh-"));
  try {
    await run(await createFixture(root));
  } finally {
    try {
      for (const pid of await startedPids(root)) {
        try {
          process.kill(pid, "SIGTERM");
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
        }
      }
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  }
}

describe("managed SSH agent with real agents", () => {
  test("an unreachable owned socket is replaced by a live agent", async () => {
    await withAgentFixture(async (fixture) => {
      await createStaleSocket(fixture.socket);
      const result = await fixture.setupShell();

      expect(result.code, result.stderr).toBe(0);
      expect(result.stdout).toBe(fixture.socket);
      expect(result.stderr).toBe("");
      expect((await fixture.expectConnected()).code).toBe(1);
      expect((await fixture.pids()).size).toBe(1);
    });
  }, 30_000);

  test("repeated initialization keeps the live keyless agent", async () => {
    await withAgentFixture(async (fixture) => {
      for (let attempt = 0; attempt < 3; attempt++) {
        const result = await fixture.setupShell();
        expect(result.code, result.stderr).toBe(0);
        expect(result.stdout).toBe(fixture.socket);
        expect(result.stderr).toBe("");
        expect((await fixture.expectConnected()).code).toBe(1);
      }
      expect((await fixture.pids()).size).toBe(1);
    });
  }, 30_000);

  test("six concurrent shells create one reachable agent", async () => {
    await withAgentFixture(async (fixture) => {
      await createStaleSocket(fixture.socket);
      const results = await Promise.all(Array.from({ length: 6 }, () => fixture.setupShell()));
      for (const result of results) {
        expect(result.code, result.stderr).toBe(0);
        expect(result.stderr).toBe("");
        expect(result.stdout).toBe(fixture.socket);
      }
      await fixture.expectConnected();
      expect((await fixture.pids()).size).toBe(1);
    });
  }, 30_000);

  test("an externally supplied socket is never replaced", async () => {
    await withAgentFixture(async (fixture) => {
      const external = path.join(fixture.root, "external.sock");
      await createStaleSocket(external);
      const identity = (await lstat(external)).ino;

      const result = await fixture.setupShell({ SSH_AUTH_SOCK: external });

      expect(result.code, result.stderr).toBe(0);
      expect(result.stdout).toBe(external);
      expect(result.stderr).toBe("");
      expect((await lstat(external)).ino).toBe(identity);
      expect((await lstat(external)).isSocket()).toBe(true);
      expect(await lstat(fixture.socket).catch((error: NodeJS.ErrnoException) => {
        if (error.code !== "ENOENT") throw error;
        return null;
      })).toBeNull();
      expect((await fixture.pids()).size).toBe(0);
    });
  }, 30_000);

  test("regular files and symlink paths are preserved", async () => {
    await withAgentFixture(async (fixture) => {
      const target = path.join(fixture.root, "unowned");
      await writeFile(target, "keep");
      for (const linked of [false, true]) {
        await rm(fixture.socket, { force: true });
        if (linked) await symlink(target, fixture.socket);
        else await writeFile(fixture.socket, "keep");

        const result = await fixture.setupShell();

        expect(result.stderr).toContain("refusing to replace");
        expect((await lstat(fixture.socket)).isSymbolicLink()).toBe(linked);
        expect(await readFile(fixture.socket, "utf8")).toBe("keep");
        expect(await readFile(target, "utf8")).toBe("keep");
        expect((await fixture.pids()).size).toBe(0);
      }
    });
  }, 30_000);

  test("a loaded identity survives reinitialization", async () => {
    await withAgentFixture(async (fixture) => {
      const initial = await fixture.setupShell();
      expect(initial.code, initial.stderr).toBe(0);
      expect(initial.stderr).toBe("");
      const key = path.join(fixture.root, "fixture-key");
      const generated = await fixture.command(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", key], {}, 15_000);
      expect(generated.code, generated.stderr).toBe(0);
      const added = await fixture.command(["ssh-add", key]);
      expect(added.code, added.stderr).toBe(0);
      const before = await fixture.command(["ssh-add", "-L"]);
      expect(before.code, before.stderr).toBe(0);
      expect(before.stdout).toStartWith("ssh-ed25519 ");
      const pids = await fixture.pids();

      const result = await fixture.setupShell();
      const after = await fixture.command(["ssh-add", "-L"]);

      expect(result.code, result.stderr).toBe(0);
      expect(result.stderr).toBe("");
      expect(after.code, after.stderr).toBe(0);
      expect(after.stdout).toBe(before.stdout);
      expect(await fixture.pids()).toEqual(pids);
      expect(pids.size).toBe(1);
    });
  }, 30_000);
});
