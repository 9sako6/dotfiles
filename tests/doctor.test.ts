import { describe, expect, test } from "bun:test";
import { chmod, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import {
  inspectHomebrew,
  inspectMise,
  inspectSystem,
  parseHomebrewDeclarationNames,
  runDoctor,
} from "../lib/doctor";
import { withTempDir } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");

describe("doctorの診断実行", () => {
  test("一つの診断が失敗しても残りを実行して全結果を表示する", async () => {
    const visited: string[] = [];
    const result = await runDoctor([
      {
        inspect: async () => {
          visited.push("deployment");
          return { findings: [], nextSteps: [], summary: "converged" };
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
          return { findings: ["missing package"], nextSteps: [], summary: "drift detected" };
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
      const proc = Bun.spawn([process.execPath, path.join(repoRoot, "bin", "doctor.ts")], {
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
      expect(stdout).toContain("[system]\n");
      expect(stdout).toContain("[homebrew]\n");
    });
  });

  test("absolute entrypoint pathは無関係なCWDから実行しても同じリポジトリを診断する", async () => {
    await withTempDir("doctor-foreign-cwd", async (tempDir) => {
      const proc = Bun.spawn([process.execPath, path.join(repoRoot, "bin", "doctor.ts")], {
        cwd: tempDir,
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
      expect(stdout).toContain("[system]\n");
      expect(stdout).toContain("[homebrew]\n");
    });
  });

  test("診断自体が成功してもfindingがあれば失敗終了と判定する", async () => {
    const result = await runDoctor([
      {
        inspect: async () => ({
          findings: ["drift detected"],
          nextSteps: [],
          summary: "1 finding",
        }),
        title: "deployment",
      },
    ]);

    expect(result.failed).toBe(true);
  });

  test("CLIはmiseの未導入と削除候補を両方診断する", async () => {
    await withTempDir("doctor-mise-command", async (tempDir) => {
      const homeDir = path.join(tempDir, "home");
      const fakeBin = path.join(tempDir, "bin");
      await makeExecutable(path.join(homeDir, ".local", "bin", "mise"), `#!/bin/sh
case "$2" in
  --missing) printf '%s\n' '{"node":[{"installed":false,"version":"24.0.0"}]}' ;;
  --prunable) printf '%s\n' '{"bun":[{"installed":true,"version":"1.3.11"}]}' ;;
  *) exit 1 ;;
esac
`);
      await makeExecutable(path.join(fakeBin, "nix"), `#!/bin/sh
case "$*" in
  *config.homebrew.brews | *config.homebrew.casks) printf '%s\n' '[]' ;;
  *) printf '%s\n' /nix/store/expected-system ;;
esac
`);
      await makeExecutable(path.join(fakeBin, "brew"), `#!/bin/sh
exit 0
`);

      const proc = Bun.spawn([process.execPath, path.join(repoRoot, "bin", "doctor.ts")], {
        cwd: repoRoot,
        env: {
          ...process.env,
          HOME: homeDir,
          PATH: `${fakeBin}:/usr/bin:/bin`,
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
      expect(stdout).toContain("[mise]\n  1 missing, 1 prunable installation(s)\n");
      expect(stdout).toContain("  warning: missing: node@24.0.0\n");
      expect(stdout).toContain("  warning: prunable: bun@1.3.11\n");
      expect(stdout).toContain("  hint: mise prune --tools\n");
    });
  });
});

describe("doctorのmise診断", () => {
  test("miseがmissingとprunableにしたインストールを列挙する", () => {
    const result = inspectMise({
      missingRaw: JSON.stringify({
        node: [
          { active: true, installed: false, version: "24.0.0" },
        ],
      }),
      prunableRaw: JSON.stringify({
        bun: [
          { active: false, installed: true, version: "1.3.11" },
        ],
        terraform: [
          { active: false, installed: true, version: "1.14.3" },
        ],
      }),
    });

    expect(result.missing).toEqual(["node@24.0.0"]);
    expect(result.prunable).toEqual(["bun@1.3.11", "terraform@1.14.3"]);
  });
});

describe("doctorのHomebrew診断", () => {
  test("nix-darwinの文字列と詳細指定からpackage名を得る", () => {
    expect(parseHomebrewDeclarationNames(JSON.stringify([
      "git",
      { name: "mas", restart_service: true },
    ]))).toEqual(new Set(["git", "mas"]));
  });

  test("名前を持たないHomebrew宣言を拒否する", () => {
    expect(() => parseHomebrewDeclarationNames(JSON.stringify([{}]))).toThrow(
      "has no package name",
    );
  });

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

describe("doctorのsystem診断", () => {
  test("期待するStoreパスと現在のsystemが一致していれば反映済みと判定する", () => {
    expect(inspectSystem({
      activeStorePath: "/nix/store/current-system",
      expectedStorePath: "/nix/store/current-system",
    })).toEqual({ status: "active" });
  });

  test("現在のsystemがなければ未反映と判定する", () => {
    expect(inspectSystem({
      activeStorePath: null,
      expectedStorePath: "/nix/store/expected-system",
    })).toEqual({ status: "missing" });
  });

  test("Storeパスが異なれば未反映と判定する", () => {
    expect(inspectSystem({
      activeStorePath: "/nix/store/current-system",
      expectedStorePath: "/nix/store/expected-system",
    })).toEqual({ status: "outdated" });
  });
});

async function makeExecutable(filePath: string, content: string) {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
  await chmod(filePath, 0o755);
}
