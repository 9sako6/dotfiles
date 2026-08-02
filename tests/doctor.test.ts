import { describe, expect, test } from "bun:test";
import path from "node:path";
import {
  inspectHomebrew,
  inspectMise,
  runDoctor,
} from "../scripts/lib/doctor";
import { withTempDir } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");

describe("doctorの診断実行", () => {
  test("一つの診断が失敗しても残りを実行して全結果を表示する", async () => {
    const visited: string[] = [];
    const result = await runDoctor([
      {
        inspect: async () => {
          visited.push("deployment");
          return { findings: [], summary: "converged" };
        },
        title: "deployment",
      },
      {
        inspect: async () => {
          visited.push("mise");
          throw new Error("mise is unavailable");
        },
        title: "mise",
      },
      {
        inspect: async () => {
          visited.push("homebrew");
          return { findings: ["missing package"], summary: "drift detected" };
        },
        title: "homebrew",
      },
    ]);

    expect(visited.sort()).toEqual(["deployment", "homebrew", "mise"]);
    expect(result.failed).toBe(true);
    expect(result.output).toBe([
      "[deployment]",
      "  converged",
      "",
      "[mise]",
      "  diagnosis failed",
      "  error: mise is unavailable",
      "",
      "[homebrew]",
      "  drift detected",
      "  warning: missing package",
    ].join("\n"));
  });

  test("診断が失敗してもCLIは全セクションを表示して失敗終了する", async () => {
    await withTempDir("doctor-command", async (tempDir) => {
      const proc = Bun.spawn([process.execPath, path.join(repoRoot, "scripts", "doctor.ts")], {
        cwd: repoRoot,
        env: {
          ...process.env,
          HOME: path.join(tempDir, "home"),
          PATH: "/usr/bin:/bin",
        },
        stderr: "pipe",
        stdout: "pipe",
      });
      const [exitCode, stderr, stdout] = await Promise.all([
        proc.exited,
        new Response(proc.stderr).text(),
        new Response(proc.stdout).text(),
      ]);

      expect(exitCode).toBe(1);
      expect(stderr).toBe("");
      expect(stdout).toContain("[deployment]\n");
      expect(stdout).toContain("[mise]\n  diagnosis failed\n");
      expect(stdout).toContain("[homebrew]\n");
    });
  });
});

describe("doctorのmise診断", () => {
  test("miseがprunableと判定したインストールを列挙する", () => {
    const result = inspectMise(JSON.stringify({
      bun: [
        { active: false, installed: true, version: "1.3.11" },
      ],
      terraform: [
        { active: false, installed: true, version: "1.14.3" },
      ],
    }));

    expect(result.prunable).toEqual(["bun@1.3.11", "terraform@1.14.3"]);
  });
});

describe("doctorのHomebrew診断", () => {
  test("宣言済み、未導入、宣言外を区別する", () => {
    const result = inspectHomebrew({
      declaredCasks: new Set(["bitwarden", "ghostty"]),
      declaredFormulae: new Set(["git"]),
      installedCasks: ["bitwarden", "codex"],
      installedFormulae: ["gh", "git"],
    });

    expect(result.missing).toEqual(["brew-cask:ghostty"]);
    expect(result.unmanaged).toEqual(["brew-cask:codex", "brew:gh"]);
  });
});
