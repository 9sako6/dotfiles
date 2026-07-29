import { describe, expect, test } from "bun:test";
import { access, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import {
  assertRemoteUninstallTargets,
  createAgentsBuildPlan,
  finalizeCompiledAgents,
  removeLocalSkill,
  restoreLockfileIfOnlyGeneratedAtChanged,
} from "../scripts/lib/agents-build";
import { withTempDir, writeTree } from "./test-helpers";

describe("finalizeCompiledAgents", () => {
  test("moves the generated AGENTS.md into the codex config and removes unsupported APM outputs", async () => {
    await withTempDir("agents-build", async (tempDir) => {
      await writeTree(tempDir, {
        "AGENTS.md": "# agents\n",
        "CLAUDE.md": "# claude\n",
        "GEMINI.md": "# gemini\n",
        ".codex/config.toml": "[mcp_servers.example]\n",
        ".mcp.json": "{}\n",
      });

      await finalizeCompiledAgents(tempDir);

      expect(await readFile(path.join(tempDir, ".codex", "AGENTS.md"), "utf8")).toBe("# agents\n");
      for (const relativePath of ["AGENTS.md", "CLAUDE.md", "GEMINI.md", ".codex/config.toml", ".mcp.json"]) {
        await expect(access(path.join(tempDir, relativePath))).rejects.toThrow();
      }
    });
  });

  test("fails when APM does not generate AGENTS.md", async () => {
    await withTempDir("agents-build", async (tempDir) => {
      await expect(finalizeCompiledAgents(tempDir)).rejects.toThrow(/AGENTS\.md/);
    });
  });

  test("restores apm.lock.yaml when only generated_at changed", async () => {
    await withTempDir("agents-build", async (tempDir) => {
      const lockPath = path.join(tempDir, "apm.lock.yaml");
      const original = "lockfile_version: '1'\ngenerated_at: 'old'\napm_version: 0.15.0\n";
      await writeFile(lockPath, "lockfile_version: '1'\ngenerated_at: 'new'\napm_version: 0.15.0\n");

      await restoreLockfileIfOnlyGeneratedAtChanged(lockPath, original);

      expect(await readFile(lockPath, "utf8")).toBe(original);
    });
  });

  test("keeps apm.lock.yaml when dependency content changed", async () => {
    await withTempDir("agents-build", async (tempDir) => {
      const lockPath = path.join(tempDir, "apm.lock.yaml");
      const original = "lockfile_version: '1'\ngenerated_at: 'old'\napm_version: 0.15.0\n";
      const changed = "lockfile_version: '1'\ngenerated_at: 'new'\napm_version: 0.16.0\n";
      await writeFile(lockPath, changed);

      await restoreLockfileIfOnlyGeneratedAtChanged(lockPath, original);

      expect(await readFile(lockPath, "utf8")).toBe(changed);
    });
  });
});

describe("createAgentsBuildPlan", () => {
  test("keeps the existing frozen install plan for the build task", () => {
    expect(createAgentsBuildPlan("build", [])).toEqual([
      { command: "apm", args: ["install", "--frozen", "--only", "apm", "--target", "claude,codex"] },
      { command: "apm", args: ["compile", "--clean", "--target", "claude,codex"] },
    ]);
  });

  test("installs the requested package before compiling agents", () => {
    expect(createAgentsBuildPlan("install", ["mattpocock/skills/foo"])).toEqual([
      { command: "apm", args: ["install", "mattpocock/skills/foo", "--target", "claude,codex"] },
      { command: "apm", args: ["compile", "--clean", "--target", "claude,codex"] },
    ]);
  });

  test("updates the requested package before compiling agents", () => {
    expect(createAgentsBuildPlan("update", ["mattpocock/skills/foo"])).toEqual([
      { command: "apm", args: ["deps", "update", "mattpocock/skills/foo"] },
      { command: "apm", args: ["compile", "--clean", "--target", "claude,codex"] },
    ]);
  });

  test("uninstalls the requested package before compiling agents", () => {
    expect(createAgentsBuildPlan("uninstall", ["mattpocock/skills/foo"])).toEqual([
      { command: "apm", args: ["uninstall", "mattpocock/skills/foo"] },
      { command: "apm", args: ["compile", "--clean", "--target", "claude,codex"] },
    ]);
  });

  test("rejects local skill sources from the remote uninstall operation", () => {
    expect(() =>
      createAgentsBuildPlan("uninstall", ["./.apm/skills/superpowers-test-driven-development"]),
    ).toThrow(/agents:remove-local superpowers-test-driven-development/);
  });

  test("falls back to the apm command behavior when no package is specified", () => {
    expect(createAgentsBuildPlan("install", [])).toEqual([
      { command: "apm", args: ["install", "--target", "claude,codex"] },
      { command: "apm", args: ["compile", "--clean", "--target", "claude,codex"] },
    ]);
  });
});

describe("removeLocalSkill", () => {
  type RunCommand = (command: string, args: string[], cwd: string) => Promise<void>;

  test("uninstalls a registered dependency before removing its source and compiling", async () => {
    await withTempDir("remove-local-skill", async (tempDir) => {
      await writeTree(tempDir, {
        "apm.yml": [
          "dependencies:",
          "  apm:",
          "  - ./.apm/skills/example-skill",
          "  - owner/remote-skill",
          "",
        ].join("\n"),
        ".apm/skills/example-skill/SKILL.md": "# Example\n",
      });
      const commands: Array<{ command: string; args: string[]; cwd: string }> = [];
      const runCommand: RunCommand = async (command, args, cwd) => {
        commands.push({ command, args, cwd });
      };

      await removeLocalSkill(tempDir, ["example-skill"], runCommand);

      expect(commands).toEqual([
        {
          command: "apm",
          args: ["uninstall", "./.apm/skills/example-skill"],
          cwd: tempDir,
        },
        {
          command: "apm",
          args: [
            "install",
            "--frozen",
            "--only",
            "apm",
            "--target",
            "claude,codex",
          ],
          cwd: tempDir,
        },
        {
          command: "apm",
          args: ["compile", "--clean", "--target", "claude,codex"],
          cwd: tempDir,
        },
      ]);
      await expect(access(path.join(tempDir, ".apm/skills/example-skill"))).rejects.toThrow();
    });
  });

  test("re-registers an already-unregistered local source so APM can clean its deployed files", async () => {
    await withTempDir("remove-local-skill", async (tempDir) => {
      await writeTree(tempDir, {
        "apm.yml": "dependencies:\n  apm:\n  - owner/remote-skill\n",
        ".apm/skills/example-skill/SKILL.md": "# Example\n",
      });
      const commands: Array<{ command: string; args: string[]; cwd: string }> = [];

      await removeLocalSkill(tempDir, ["example-skill"], async (command, args, cwd) => {
        commands.push({ command, args, cwd });
      });

      expect(commands).toEqual([
        {
          command: "apm",
          args: [
            "install",
            "./.apm/skills/example-skill",
            "--target",
            "claude,codex",
          ],
          cwd: tempDir,
        },
        {
          command: "apm",
          args: ["uninstall", "./.apm/skills/example-skill"],
          cwd: tempDir,
        },
        {
          command: "apm",
          args: [
            "install",
            "--frozen",
            "--only",
            "apm",
            "--target",
            "claude,codex",
          ],
          cwd: tempDir,
        },
        {
          command: "apm",
          args: ["compile", "--clean", "--target", "claude,codex"],
          cwd: tempDir,
        },
      ]);
      await expect(access(path.join(tempDir, ".apm/skills/example-skill"))).rejects.toThrow();
    });
  });

  test("rejects unsafe names and missing local skills before running commands", async () => {
    await withTempDir("remove-local-skill", async (tempDir) => {
      await writeTree(tempDir, { "apm.yml": "dependencies:\n  apm: []\n" });
      const commands: string[] = [];
      const runCommand: RunCommand = async (command) => {
        commands.push(command);
      };
      await expect(removeLocalSkill(tempDir, ["../outside"], runCommand)).rejects.toThrow(/skill name/);
      await expect(removeLocalSkill(tempDir, ["missing-skill"], runCommand)).rejects.toThrow(/not found/);
      expect(commands).toEqual([]);
    });
  });

  test("preserves the source when APM cannot uninstall the registered dependency", async () => {
    await withTempDir("remove-local-skill", async (tempDir) => {
      await writeTree(tempDir, {
        "apm.yml": "dependencies:\n  apm:\n  - ./.apm/skills/example-skill\n",
        ".apm/skills/example-skill/SKILL.md": "# Example\n",
      });

      await expect(
        removeLocalSkill(tempDir, ["example-skill"], async () => {
          throw new Error("APM failed");
        }),
      ).rejects.toThrow("APM failed");

      expect(await readFile(path.join(tempDir, ".apm/skills/example-skill/SKILL.md"), "utf8")).toBe(
        "# Example\n",
      );
    });
  });
});

describe("assertRemoteUninstallTargets", () => {
  test("rejects the bare name of a repository-owned local skill", async () => {
    await withTempDir("remote-uninstall", async (tempDir) => {
      await writeTree(tempDir, {
        ".apm/skills/example-skill/SKILL.md": "# Example\n",
      });

      await expect(
        assertRemoteUninstallTargets(tempDir, ["example-skill"]),
      ).rejects.toThrow(/agents:remove-local example-skill/);
    });
  });

  test("allows remote package identifiers", async () => {
    await withTempDir("remote-uninstall", async (tempDir) => {
      await expect(
        assertRemoteUninstallTargets(tempDir, ["owner/remote-skill"]),
      ).resolves.toBeUndefined();
    });
  });
});
