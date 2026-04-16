import { chmod, mkdir, stat, writeFile } from "node:fs/promises";
import { describe, expect, test } from "bun:test";
import { spawn } from "node:child_process";
import path from "node:path";
import { withTempDir } from "./test-helpers";
import { buildSetupPlan, syncSkills } from "../scripts/lib/setup";

function runCommand(
  command: string,
  args: string[],
  env: NodeJS.ProcessEnv = process.env,
  options: { cwd?: string } = {},
) {
  return new Promise<{ code: number | null; stderr: string; stdout: string }>((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += String(chunk);
    });
    child.stderr.on("data", (chunk) => {
      stderr += String(chunk);
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      resolve({ code, stderr, stdout });
    });
  });
}

async function writeInstallZinitRunner(scriptPath: string) {
  await writeFile(
    scriptPath,
    `import { installZinit } from ${JSON.stringify(path.join(process.cwd(), "scripts", "lib", "setup.ts"))};

const homeDir = process.env.TEST_HOME_DIR;
if (!homeDir) {
  throw new Error("TEST_HOME_DIR is not set");
}

await installZinit(homeDir);
`,
  );
}

describe("installZinit", () => {
  test("fetches only the pinned commit into a fresh repository", async () => {
    await withTempDir("install-zinit", async (tempDir) => {
      const homeDir = path.join(tempDir, "home");
      const binDir = path.join(tempDir, "bin");
      const commandLogPath = path.join(tempDir, "git-commands.log");
      const runnerPath = path.join(tempDir, "run-install-zinit.ts");
      const zinitDir = path.join(homeDir, ".local", "share", "zinit", "zinit.git");

      await Promise.all([mkdir(binDir, { recursive: true }), writeInstallZinitRunner(runnerPath)]);
      await writeFile(
        path.join(binDir, "git"),
        `#!/bin/sh
set -eu
printf '%s\\n' "$*" >> "${commandLogPath}"
if [ "$1" = "init" ]; then
  mkdir -p "$2"
fi
`,
      );
      await chmod(path.join(binDir, "git"), 0o755);

      const result = await runCommand(process.execPath, [runnerPath], {
        ...process.env,
        PATH: `${binDir}:${process.env.PATH ?? ""}`,
        TEST_HOME_DIR: homeDir,
      });

      expect(result.code).toBe(0);
      expect(result.stderr).toBe("");
      expect(await stat(zinitDir, { throwIfNoEntry: false })).toBeDefined();
      expect(await Bun.file(commandLogPath).text()).toBe(
        [
          `init ${zinitDir}`,
          `-C ${zinitDir} remote add origin https://github.com/zdharma-continuum/zinit.git`,
          `-C ${zinitDir} fetch --depth=1 origin 55d19f86f627c9995db9885d0971d9b6701fe0d3`,
          `-C ${zinitDir} checkout --detach FETCH_HEAD`,
          "",
        ].join("\n"),
      );
    });
  });

  test("fails when the install target already exists", async () => {
    await withTempDir("install-zinit-exists", async (tempDir) => {
      const homeDir = path.join(tempDir, "home");
      const binDir = path.join(tempDir, "bin");
      const commandLogPath = path.join(tempDir, "git-commands.log");
      const runnerPath = path.join(tempDir, "run-install-zinit.ts");
      const zinitDir = path.join(homeDir, ".local", "share", "zinit", "zinit.git");

      await Promise.all([
        mkdir(zinitDir, { recursive: true }),
        mkdir(binDir, { recursive: true }),
        writeInstallZinitRunner(runnerPath),
      ]);
      await writeFile(
        path.join(binDir, "git"),
        `#!/bin/sh
set -eu
printf '%s\\n' "$*" >> "${commandLogPath}"
exit 0
`,
      );
      await chmod(path.join(binDir, "git"), 0o755);

      const result = await runCommand(process.execPath, [runnerPath], {
        ...process.env,
        PATH: `${binDir}:${process.env.PATH ?? ""}`,
        TEST_HOME_DIR: homeDir,
      });

      expect(result.code).not.toBe(0);
      expect(result.stderr).toContain("zinit install target already exists");
      expect(await stat(commandLogPath, { throwIfNoEntry: false })).toBeUndefined();
    });
  });
});

describe("buildSetupPlan", () => {
  const baseOptions = {
    brewInstalled: false,
    homeDir: "/home/me",
    platform: "linux" as NodeJS.Platform,
    repoRoot: "/repo",
    skipExtraSetup: false,
    zinitInstalled: true,
  };

  test("inserts sync-skills immediately after link-dist", () => {
    const steps = buildSetupPlan(baseOptions);
    const kinds = steps.map((step) => step.kind);
    const linkIndex = kinds.indexOf("link-dist");
    const syncIndex = kinds.indexOf("sync-skills");

    expect(linkIndex).toBeGreaterThanOrEqual(0);
    expect(syncIndex).toBe(linkIndex + 1);
  });

  test("sync-skills carries the absolute dist path", () => {
    const steps = buildSetupPlan(baseOptions);
    const syncStep = steps.find((step) => step.kind === "sync-skills");

    expect(syncStep).toBeDefined();
    expect(syncStep).toMatchObject({
      kind: "sync-skills",
      distPath: "/repo/dist",
    });
  });

  test("omits sync-skills when skipExtraSetup is true", () => {
    const steps = buildSetupPlan({ ...baseOptions, skipExtraSetup: true });
    expect(steps.map((step) => step.kind)).not.toContain("sync-skills");
  });
});

describe("syncSkills", () => {
  test("invokes the repo-managed skills CLI with expected arguments", async () => {
    const calls: { command: string; args: string[] }[] = [];
    const runner = async (command: string, args: string[]) => {
      calls.push({ command, args });
    };

    await syncSkills("/repo/dist", runner);

    expect(calls).toEqual([
      {
        command: "skills",
        args: [
          "add",
          "/repo/dist",
          "--copy",
          "--global",
          "--agent",
          "claude-code",
          "--agent",
          "codex",
          "--skill",
          "*",
          "--yes",
        ],
      },
    ]);
  });

  test("repo mise config pins skills via the npm backend", async () => {
    const result = await runCommand("mise", ["ls", "--json", "npm:skills"], process.env, {
      cwd: process.cwd(),
    });

    expect(result.code).toBe(0);
    expect(result.stderr).toBe("");
    const [skillsTool] = JSON.parse(result.stdout);

    expect(skillsTool).toMatchObject({
      requested_version: "1.5.0",
      source: {
        path: path.join(process.cwd(), ".mise.toml"),
        type: "mise.toml",
      },
      version: "1.5.0",
    });
  });
});
