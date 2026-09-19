import { describe, expect, test } from "bun:test";
import { chmod, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");
const installSystemLibrary = path.join(repoRoot, "lib", "install-system.sh");
const systemBackend = path.join(repoRoot, "bin", "system-backend.sh");

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

test("preview skips outdated packages while retaining missing installations", async () => {
  await withTempDir("homebrew-no-upgrade-plan", async (tempDir) => {
    const brewBin = path.join(tempDir, "brew");
    await makeExecutable(brewBin, `#!/bin/sh
case "$*" in
  "list --formula --full-name") exit 0 ;;
  "list --cask --full-name") printf '%s\\n' bitwarden; exit 0 ;;
esac
case " $* " in
  *" --no-upgrade "*) ;;
  *) printf '%s\\n' '→ Cask bitwarden needs to be installed or updated.' ;;
esac
printf '%s\\n' '→ Cask ghostty needs to be installed or updated.'
exit 1
`);
    const result = await runHomebrewMissingPlan(brewBin, 1, "");
    expect(result).toEqual({
      exitCode: 0,
      stderr: "",
      stdout: "  + Install ghostty (cask)\n",
    });
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

type Declarations = { formula: string[]; cask: string[]; tap: string[] };

async function runConfigurationPreview({
  installed = false,
  active = true,
  previous = { formula: ["libyaml"], cask: ["cryptomator", "ghostty"], tap: [] },
  desired = { formula: ["libyaml"], cask: ["ghostty"], tap: [] },
  failure = "",
}: {
  installed?: boolean;
  active?: boolean;
  previous?: Declarations;
  desired?: Declarations;
  failure?: "" | "references" | "multiple" | "previous" | "planned";
} = {}) {
  return withTempDir("homebrew-configuration-preview", async (tempDir) => {
    const fakeBin = path.join(tempDir, "bin");
    const currentSystem = path.join(tempDir, "current-system");
    const previousBrewfile = path.join(tempDir, "previous-Brewfile");
    const desiredBrewfile = path.join(tempDir, "planned-Brewfile");
    await mkdir(fakeBin);
    if (active) await mkdir(currentSystem);
    for (const [file, entries] of [[previousBrewfile, previous], [desiredBrewfile, desired]] as const) {
      const lines = [];
      for (const kind of ["formula", "cask", "tap"] as const) {
        await writeFile(`${file}.${kind}`, entries[kind].join("\n"));
        for (const name of entries[kind]) {
          lines.push(`${kind === "formula" ? "brew" : kind} ${JSON.stringify(name)}`);
        }
      }
      await writeFile(file, lines.join("\n"));
    }
    await makeExecutable(path.join(fakeBin, "nix"), "#!/bin/sh\nexit 0\n");
    await makeExecutable(path.join(fakeBin, "nix-store"), `#!/bin/sh
[ "$PREVIEW_FAILURE" != references ] || exit 2
printf '%s\\n' "$PREVIOUS_BREWFILE"
if [ "$PREVIEW_FAILURE" = multiple ]; then printf '%s\\n' "$PLANNED_BREWFILE"; fi
`);
    await makeExecutable(path.join(fakeBin, "brew"), `#!/bin/sh
case "$1:$2:$3" in
  bundle:list:--formula|bundle:list:--cask|bundle:list:--tap)
    [ "$4" = --file ] || exit 2
    if [ "$PREVIEW_FAILURE" = previous ] && [ "$5" = "$PREVIOUS_BREWFILE" ]; then exit 2; fi
    if [ "$PREVIEW_FAILURE" = planned ] && [ "$5" = "$PLANNED_BREWFILE" ]; then exit 2; fi
    cat "$5.\${3#--}"
    ;;
  bundle:check:--verbose) exit 0 ;;
  list:--formula:--full-name) cat "$PLANNED_BREWFILE.formula" ;;
  list:--cask:--full-name)
    cat "$PLANNED_BREWFILE.cask"
    if [ "$CRYPTOMATOR_INSTALLED" = yes ]; then printf '\\n%s\\n' cryptomator; fi
    ;;
  bundle:cleanup:--file)
    if [ "$CRYPTOMATOR_INSTALLED" = yes ]; then
      printf '%s\\n' 'Would uninstall casks:' cryptomator
      exit 1
    fi
    ;;
  *) exit 2 ;;
esac
`);
    const process = Bun.spawn([
      "/bin/sh", systemBackend, "preview", path.join(fakeBin, "nix"),
      path.join(tempDir, "planned-system"), desiredBrewfile, currentSystem,
    ], {
      env: {
        ...Bun.env,
        PATH: `${fakeBin}:/usr/bin:/bin`,
        CRYPTOMATOR_INSTALLED: installed ? "yes" : "no",
        PLANNED_BREWFILE: desiredBrewfile,
        PREVIOUS_BREWFILE: previousBrewfile,
        PREVIEW_FAILURE: failure,
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
  });
}

test.each([false, true])("shows removed Cryptomator declaration when installed=%s", async (installed) => {
  const result = await runConfigurationPreview({ installed });
  expect(result.exitCode).toBe(0);
  expect(result.stderr).toBe("");
  expect(result.stdout).toContain("Homebrew configuration changes:\n  - cryptomator (cask)\n");
  if (installed) {
    expect(result.stdout).toContain("Would uninstall casks:\ncryptomator\n");
  } else {
    expect(result.stdout).toContain("Homebrew cleanup candidates:\n  No cleanup candidates\n");
    expect(result.stdout).not.toContain("Would uninstall");
  }
});

test("compares formula, cask and tap declarations without treating order as a change", async () => {
  const result = await runConfigurationPreview({
    previous: { formula: ["kept", "retired"], cask: ["second", "first"], tap: ["example/old"] },
    desired: { formula: ["added", "kept"], cask: ["first", "second"], tap: ["example/new"] },
  });
  expect(result.exitCode).toBe(0);
  expect(result.stdout).toContain([
    "Homebrew configuration changes:",
    "  + added (formula)",
    "  - retired (formula)",
    "  + example/new (tap)",
    "  - example/old (tap)",
  ].join("\n"));
  expect(result.stdout).not.toContain("(cask)");
});

test("omits declaration changes when the active and planned configuration agree", async () => {
  const entries = { formula: ["libyaml"], cask: ["ghostty"], tap: [] };
  const result = await runConfigurationPreview({ previous: entries, desired: entries });
  expect(result.exitCode).toBe(0);
  expect(result.stdout).not.toContain("Homebrew configuration changes:");
});

test("can preview a first installation without an active system", async () => {
  const result = await runConfigurationPreview({ active: false, failure: "references" });
  expect(result.exitCode).toBe(0);
  expect(result.stdout).toContain("system diff: no active nix-darwin generation");
  expect(result.stdout).not.toContain("Homebrew configuration changes:");
});

test.each(["references", "multiple", "previous", "planned"] as const)("refuses a misleading configuration comparison after %s failure", async (failure) => {
  const result = await runConfigurationPreview({ failure });
  expect(result.exitCode).not.toBe(0);
  expect(result.stderr).toContain("system:");
  expect(result.stdout).not.toContain("No cleanup candidates");
});
