import { describe, expect, test } from "bun:test";
import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");
const installSystem = path.join(repoRoot, "scripts/install-system.sh");

async function makeExecutable(filePath: string, content: string) {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
  await chmod(filePath, 0o755);
}

async function runInstallSystem(fakeBin: string, env: Record<string, string>) {
  const proc = Bun.spawn(["/bin/sh", installSystem], {
    cwd: repoRoot,
    env: {
      ...process.env,
      ...env,
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
  return { exitCode, stderr, stdout };
}

describe("install:system", () => {
  test("applies the locked darwin configuration with runtime-local identity", async () => {
    await withTempDir("install-system", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const logPath = path.join(tempDir, "system.log");

      await makeExecutable(
        path.join(fakeBin, "uname"),
        `#!/bin/sh
case "$1" in
  -s) printf '%s\\n' Darwin ;;
  -m) printf '%s\\n' arm64 ;;
esac
`,
      );
      await makeExecutable(
        path.join(fakeBin, "id"),
        `#!/bin/sh
case "$1" in
  -u) printf '%s\\n' 501 ;;
  -un) printf '%s\\n' test-user ;;
esac
`,
      );
      await makeExecutable(
        path.join(fakeBin, "sudo"),
        `#!/bin/sh
exec "$@"
`,
      );
      await makeExecutable(
        path.join(fakeBin, "nix"),
        `#!/bin/sh
{
  printf 'primary_user=%s\\n' "$DARWIN_PRIMARY_USER"
  printf 'args='
  printf '<%s>' "$@"
  printf '\\n'
} > "$SYSTEM_INSTALL_LOG"
`,
      );

      const result = await runInstallSystem(fakeBin, {
        SYSTEM_INSTALL_LOG: logPath,
      });

      expect(result).toMatchObject({ exitCode: 0, stderr: "", stdout: "" });
      const log = await readFile(logPath, "utf8");
      expect(log).toContain("primary_user=test-user");
      expect(log).toContain("<--extra-experimental-features><nix-command flakes>");
      expect(log).toContain("<run><--impure>");
      expect(log).toMatch(/<path:[^>]+\/darwin#darwin-rebuild>/);
      expect(log).toContain("<switch><--flake>");
      expect(log).toMatch(/<path:[^>]+\/darwin#aarch64-darwin><--impure>/);
    });
  });

  test("rejects unsupported architectures before downloading anything", async () => {
    await withTempDir("install-system-arch", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const downloadMarker = path.join(tempDir, "downloaded");

      await makeExecutable(
        path.join(fakeBin, "uname"),
        `#!/bin/sh
case "$1" in
  -s) printf '%s\\n' Darwin ;;
  -m) printf '%s\\n' x86_64 ;;
esac
`,
      );
      await makeExecutable(
        path.join(fakeBin, "curl"),
        `#!/bin/sh
touch "$SYSTEM_DOWNLOAD_MARKER"
`,
      );

      const result = await runInstallSystem(fakeBin, {
        SYSTEM_DOWNLOAD_MARKER: downloadMarker,
      });

      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("supports Apple Silicon only");
      expect(await Bun.file(downloadMarker).exists()).toBe(false);
    });
  });

  test("does not execute a downloaded installer when its checksum differs", async () => {
    await withTempDir("install-system-checksum", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const installerMarker = path.join(tempDir, "installer-ran");
      const sudoMarker = path.join(tempDir, "sudo-ran");

      await makeExecutable(
        path.join(fakeBin, "uname"),
        `#!/bin/sh
case "$1" in
  -s) printf '%s\\n' Darwin ;;
  -m) printf '%s\\n' arm64 ;;
esac
`,
      );
      await makeExecutable(
        path.join(fakeBin, "id"),
        `#!/bin/sh
case "$1" in
  -u) printf '%s\\n' 501 ;;
  -un) printf '%s\\n' test-user ;;
esac
`,
      );
      await makeExecutable(
        path.join(fakeBin, "curl"),
        `#!/bin/sh
output=
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--output" ]; then
    shift
    output="$1"
  fi
  shift
done
printf '%s\\n' '#!/bin/sh' "touch \\"$SYSTEM_INSTALLER_MARKER\\"" > "$output"
`,
      );
      await makeExecutable(
        path.join(fakeBin, "shasum"),
        `#!/bin/sh
exit 1
`,
      );
      await makeExecutable(
        path.join(fakeBin, "sudo"),
        `#!/bin/sh
touch "$SYSTEM_SUDO_MARKER"
`,
      );

      const result = await runInstallSystem(fakeBin, {
        SYSTEM_INSTALLER_MARKER: installerMarker,
        SYSTEM_SUDO_MARKER: sudoMarker,
      });

      expect(result.exitCode).not.toBe(0);
      expect(await Bun.file(installerMarker).exists()).toBe(false);
      expect(await Bun.file(sudoMarker).exists()).toBe(false);
    });
  });
});
