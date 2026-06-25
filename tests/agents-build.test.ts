import { describe, expect, test } from "bun:test";
import { access, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { createAgentsBuildPlan, finalizeCompiledAgents, restoreLockfileIfOnlyGeneratedAtChanged } from "../scripts/lib/agents-build";
import { withTempDir, writeTree } from "./test-helpers";

describe("finalizeCompiledAgents", () => {
  test("moves the generated AGENTS.md into the codex config and removes unsupported APM outputs", async () => {
    await withTempDir("agents-build", async (tempDir) => {
      await writeTree(tempDir, {
        "AGENTS.md": "# agents\n",
        "CLAUDE.md": "# claude\n",
        "GEMINI.md": "# gemini\n",
        ".codex/config.toml": "[mcp_servers.playwright]\n",
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

  test("falls back to the apm command behavior when no package is specified", () => {
    expect(createAgentsBuildPlan("install", [])).toEqual([
      { command: "apm", args: ["install", "--target", "claude,codex"] },
      { command: "apm", args: ["compile", "--clean", "--target", "claude,codex"] },
    ]);
  });
});
