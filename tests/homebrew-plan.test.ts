import { describe, expect, test } from "bun:test";
import { chmod, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");
const installSystemLibrary = path.join(repoRoot, "lib", "install-system.sh");

async function makeExecutable(filePath: string, content: string) {
  await writeFile(filePath, content);
  await chmod(filePath, 0o755);
}

async function runHomebrewMissingPlan(
  brewBin: string,
  exitStatus: number,
  output: string,
  formulae = "",
  casks = "",
) {
  const process = Bun.spawn([
    "/bin/sh",
    "-c",
    '. "$1"; install_system_show_homebrew_missing "$2" /Brewfile',
    "homebrew-plan-test",
    installSystemLibrary,
    brewBin,
  ], {
    env: {
      ...Bun.env,
      INSTALLED_FORMULAE: formulae,
      INSTALLED_CASKS: casks,
      BREW_EXIT_STATUS: String(exitStatus),
      BREW_OUTPUT: output,
    },
    stderr: "pipe",
    stdout: "pipe",
  });
  const [exitCode, stderr, stdout] = await Promise.all([
    process.exited,
    new Response(process.stderr).text(),
    new Response(process.stdout).text(),
  ]);
  return { exitCode, stderr, stdout };
}

describe("Homebrew dependency plan", () => {
  test("未導入dependencyを表示し、候補ありのstatusを成功として扱う", async () => {
    await withTempDir("homebrew-missing-plan", async (tempDir) => {
      const brewBin = path.join(tempDir, "brew");
      await makeExecutable(brewBin, `#!/bin/sh
case "$*" in
  "list --formula --full-name") printf '%s\\n' "$INSTALLED_FORMULAE"; exit 0 ;;
  "list --cask --full-name") printf '%s\\n' "$INSTALLED_CASKS"; exit 0 ;;
esac
printf '%s\n' "$BREW_OUTPUT"
exit "$BREW_EXIT_STATUS"
`);

      const result = await runHomebrewMissingPlan(
        brewBin,
        1,
        "Missing dependencies:\nghostty",
      );

      expect(result).toEqual({
        exitCode: 0,
        stderr: "",
        stdout: "Missing dependencies:\nghostty\n",
      });
    });
  });

  test("不足がなく出力もなければnoneを表示する", async () => {
    await withTempDir("homebrew-complete-plan", async (tempDir) => {
      const brewBin = path.join(tempDir, "brew");
      await makeExecutable(brewBin, `#!/bin/sh
case "$*" in
  "list --formula --full-name") printf '%s\\n' "$INSTALLED_FORMULAE"; exit 0 ;;
  "list --cask --full-name") printf '%s\\n' "$INSTALLED_CASKS"; exit 0 ;;
esac
exit "$BREW_EXIT_STATUS"
`);

      const result = await runHomebrewMissingPlan(brewBin, 0, "");

      expect(result).toEqual({ exitCode: 0, stderr: "", stdout: "  No package changes\n" });
    });
  });

  test("bundle check自体の失敗はplan失敗として扱う", async () => {
    await withTempDir("homebrew-plan-failure", async (tempDir) => {
      const brewBin = path.join(tempDir, "brew");
      await makeExecutable(brewBin, `#!/bin/sh
case "$*" in
  "list --formula --full-name") printf '%s\\n' "$INSTALLED_FORMULAE"; exit 0 ;;
  "list --cask --full-name") printf '%s\\n' "$INSTALLED_CASKS"; exit 0 ;;
esac
printf '%s\n' "$BREW_OUTPUT" >&2
exit "$BREW_EXIT_STATUS"
`);

      const result = await runHomebrewMissingPlan(brewBin, 2, "unexpected failure");

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("Homebrew dependency plan failed");
    });
  });
});

test("distinguishes new casks from installed formula and cask updates", async () => {
  await withTempDir("homebrew-package-actions", async (tempDir) => {
    const brewBin = path.join(tempDir, "brew");
    await makeExecutable(brewBin, `#!/bin/sh
case "$*" in
  "list --formula --full-name") printf '%s\\n' "$INSTALLED_FORMULAE"; exit 0 ;;
  "list --cask --full-name") printf '%s\\n' "$INSTALLED_CASKS"; exit 0 ;;
esac
printf '%s\\n' "$BREW_OUTPUT"
exit "$BREW_EXIT_STATUS"
`);
    const result = await runHomebrewMissingPlan(brewBin, 1, [
      "brew bundle can't satisfy your Brewfile's dependencies.",
      "→ Cask cryptomator needs to be installed or updated.",
      "→ Cask tinycast needs to be installed or updated.",
      "→ Formula steipete/tap/remindctl needs to be installed or updated.",
      "→ Formula libyaml needs to be installed or updated.",
      "Satisfy missing dependencies with `brew bundle install`.",
    ].join("\n"), "other-formula\nsteipete/tap/remindctl", "bitwarden\nabue-ammar/tinycast/tinycast");
    expect(result).toEqual({exitCode: 0, stderr: "", stdout: [
      "  + Install cryptomator (cask)",
      "  ~ Update tinycast (cask)",
      "  ~ Update steipete/tap/remindctl (formula)",
      "  + Install libyaml (formula)",
      "",
    ].join("\n")});
  });
});

test("installed inventory failure does not mislabel updates as installs", async () => {
  await withTempDir("homebrew-inventory-failure", async (tempDir) => {
    const brewBin = path.join(tempDir, "brew");
    await makeExecutable(brewBin, `#!/bin/sh
case "$1" in
  list) exit 2 ;;
esac
exit 0
`);
    const result = await runHomebrewMissingPlan(brewBin, 0, "");
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("installed formula inspection failed");
  });
});

test("summarizes caches while retaining removals and unrelated warnings", async () => {
  await withTempDir("homebrew-cleanup-summary", async (tempDir) => {
    const brewBin = path.join(tempDir, "brew");
    await makeExecutable(brewBin, `#!/bin/sh
cat <<'OUTPUT'
Would uninstall casks:
obsolete-app
Warning: Skipping steipete/tap/remindctl: most recent version 0.3.6 not installed
Warning: another problem
Would \`brew cleanup\`:
Would remove: /cache/first (13KB)
Would remove: /cache/second (20KB)
Run \`brew bundle cleanup --force\` to make these changes.
OUTPUT
exit 1
`);
    const process = Bun.spawn(["/bin/sh", "-c",
      '. "$1"; install_system_show_homebrew_cleanup "$2" /Brewfile',
      "cleanup-test", installSystemLibrary, brewBin,
    ], {stdout: "pipe", stderr: "pipe"});
    const stdout = await new Response(process.stdout).text();
    expect(await process.exited).toBe(0);
    expect(stdout).toBe([
      "Would uninstall casks:", "obsolete-app",
      "Warning: Skipping steipete/tap/remindctl: most recent version 0.3.6 not installed",
      "Warning: another problem",
      "  Cache and old-version cleanup: 2 entries", "",
    ].join("\n"));
  });
});
