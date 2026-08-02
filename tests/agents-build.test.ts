import { describe, expect, test } from "bun:test";
import { access, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import {
  assertRemoteUninstallTargets,
  createAgentsBuildPlan,
  finalizeCompiledAgents,
  removeLocalSkill,
  restoreLockfileIfOnlyGeneratedAtChanged,
} from "../lib/agents-build";
import { withTempDir, writeTree } from "./test-helpers";

describe("生成したエージェント設定の後処理", () => {
  test("生成したAGENTS.mdをCodex設定へ移し、APMの未対応出力を削除する", async () => {
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

  test("APMがAGENTS.mdを生成しなければ失敗する", async () => {
    await withTempDir("agents-build", async (tempDir) => {
      await expect(finalizeCompiledAgents(tempDir)).rejects.toThrow(/AGENTS\.md/);
    });
  });

  test("generated_atだけが変わったapm.lock.yamlを元に戻す", async () => {
    await withTempDir("agents-build", async (tempDir) => {
      const lockPath = path.join(tempDir, "apm.lock.yaml");
      const original = "lockfile_version: '1'\ngenerated_at: 'old'\napm_version: 0.15.0\n";
      await writeFile(lockPath, "lockfile_version: '1'\ngenerated_at: 'new'\napm_version: 0.15.0\n");

      await restoreLockfileIfOnlyGeneratedAtChanged(lockPath, original);

      expect(await readFile(lockPath, "utf8")).toBe(original);
    });
  });

  test("依存関係が変わったapm.lock.yamlは残す", async () => {
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

describe("エージェント設定の生成手順", () => {
  test("ビルド時は既存の固定インストール手順を使う", () => {
    expect(createAgentsBuildPlan("build", [])).toEqual([
      { command: "apm", args: ["install", "--frozen", "--only", "apm", "--target", "claude,codex"] },
      { command: "apm", args: ["compile", "--clean", "--target", "claude,codex"] },
    ]);
  });

  test("指定したパッケージをインストールしてからエージェント設定を生成する", () => {
    expect(createAgentsBuildPlan("install", ["mattpocock/skills/foo"])).toEqual([
      { command: "apm", args: ["install", "mattpocock/skills/foo", "--target", "claude,codex"] },
      { command: "apm", args: ["compile", "--clean", "--target", "claude,codex"] },
    ]);
  });

  test("指定したパッケージを更新してからエージェント設定を生成する", () => {
    expect(createAgentsBuildPlan("update", ["mattpocock/skills/foo"])).toEqual([
      { command: "apm", args: ["deps", "update", "mattpocock/skills/foo"] },
      { command: "apm", args: ["compile", "--clean", "--target", "claude,codex"] },
    ]);
  });

  test("指定したパッケージをアンインストールしてからエージェント設定を生成する", () => {
    expect(createAgentsBuildPlan("uninstall", ["mattpocock/skills/foo"])).toEqual([
      { command: "apm", args: ["uninstall", "mattpocock/skills/foo"] },
      { command: "apm", args: ["compile", "--clean", "--target", "claude,codex"] },
    ]);
  });

  test("リモート用のアンインストール操作ではローカルスキルを拒否する", () => {
    expect(() =>
      createAgentsBuildPlan("uninstall", ["./.apm/skills/superpowers-test-driven-development"]),
    ).toThrow(/agents:remove-local superpowers-test-driven-development/);
  });

  test("パッケージを指定しなくてもAPMのインストールと生成を実行する", () => {
    expect(createAgentsBuildPlan("install", [])).toEqual([
      { command: "apm", args: ["install", "--target", "claude,codex"] },
      { command: "apm", args: ["compile", "--clean", "--target", "claude,codex"] },
    ]);
  });
});

describe("ローカルスキルの削除", () => {
  type RunCommand = (command: string, args: string[], cwd: string) => Promise<void>;

  test("登録済みの依存を解除してからソースを削除し、エージェント設定を再生成する", async () => {
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

  test("登録解除済みのローカルソースを再登録し、APMで配備済みファイルを削除する", async () => {
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

  test("コマンド実行前に危険な名前と存在しないローカルスキルを拒否する", async () => {
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

  test("APMが登録済みの依存を解除できなければソースを残す", async () => {
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

  test("登録解除後の再生成に失敗したらローカルソースを残す", async () => {
    await withTempDir("remove-local-skill", async (tempDir) => {
      await writeTree(tempDir, {
        "apm.yml": "dependencies:\n  apm:\n  - ./.apm/skills/example-skill\n",
        ".apm/skills/example-skill/SKILL.md": "# Example\n",
      });
      let commandCount = 0;

      await expect(
        removeLocalSkill(tempDir, ["example-skill"], async () => {
          commandCount += 1;
          if (commandCount === 3) {
            throw new Error("compile failed");
          }
        }),
      ).rejects.toThrow("compile failed");

      expect(commandCount).toBe(3);
      expect(await readFile(path.join(tempDir, ".apm/skills/example-skill/SKILL.md"), "utf8"))
        .toBe("# Example\n");
    });
  });
});

describe("リモートパッケージのアンインストール対象", () => {
  test("リポジトリ管理のローカルスキルを名前だけで指定した場合は拒否する", async () => {
    await withTempDir("remote-uninstall", async (tempDir) => {
      await writeTree(tempDir, {
        ".apm/skills/example-skill/SKILL.md": "# Example\n",
      });

      await expect(
        assertRemoteUninstallTargets(tempDir, ["example-skill"]),
      ).rejects.toThrow(/agents:remove-local example-skill/);
    });
  });

  test("リモートパッケージの識別子は許可する", async () => {
    await withTempDir("remote-uninstall", async (tempDir) => {
      await expect(
        assertRemoteUninstallTargets(tempDir, ["owner/remote-skill"]),
      ).resolves.toBeUndefined();
    });
  });
});
