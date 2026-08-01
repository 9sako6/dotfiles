import { describe, expect, test } from "bun:test";
import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");
const installLix = path.join(repoRoot, "scripts/install-lix.sh");
const installMise = path.join(repoRoot, "scripts/install-mise.sh");
const installSystem = path.join(repoRoot, "scripts/install-system.sh");

async function makeExecutable(filePath: string, content: string) {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
  await chmod(filePath, 0o755);
}

async function runScript(
  script: string,
  fakeBin: string,
  env: Record<string, string>,
) {
  const proc = Bun.spawn(["/bin/sh", script], {
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

describe("install:mise", () => {
  test("指定バージョンが導入済みなら再導入しない", async () => {
    await withTempDir("install-mise-current", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const miseBin = path.join(tempDir, ".local/bin/mise");
      const downloadMarker = path.join(tempDir, "downloaded");

      await makeExecutable(
        miseBin,
        `#!/bin/sh
printf '%s\n' '2026.7.7 macos-arm64'
`,
      );
      await makeExecutable(
        path.join(fakeBin, "curl"),
        `#!/bin/sh
touch "$MISE_DOWNLOAD_MARKER"
exit 1
`,
      );

      const result = await runScript(installMise, fakeBin, {
        HOME: tempDir,
        MISE_DOWNLOAD_MARKER: downloadMarker,
      });

      expect(result).toMatchObject({ exitCode: 0, stderr: "", stdout: "" });
      expect(await Bun.file(downloadMarker).exists()).toBe(false);
    });
  });

  test("未導入なら固定バージョンを導入する", async () => {
    await withTempDir("install-mise-missing", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const installMarker = path.join(tempDir, "installed-version");

      await makeExecutable(
        path.join(fakeBin, "curl"),
        `#!/bin/sh
printf '%s\n' '#!/bin/sh' 'printf "%s\\n" "$MISE_VERSION" > "$MISE_INSTALL_MARKER"'
`,
      );

      const result = await runScript(installMise, fakeBin, {
        HOME: tempDir,
        MISE_INSTALL_MARKER: installMarker,
      });

      expect(result).toMatchObject({ exitCode: 0, stderr: "", stdout: "" });
      expect(await readFile(installMarker, "utf8")).toBe("v2026.7.7\n");
    });
  });
});

describe("install:system", () => {
  test("実行時に取得したユーザー名を nix-darwin へ渡す", async () => {
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

      const result = await runScript(installSystem, fakeBin, {
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

  test("非対応の CPU ではダウンロード前に終了する", async () => {
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

      const result = await runScript(installSystem, fakeBin, {
        SYSTEM_DOWNLOAD_MARKER: downloadMarker,
      });

      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("supports Apple Silicon only");
      expect(await Bun.file(downloadMarker).exists()).toBe(false);
    });
  });

  test("チェックサムが一致しない Lix Installer を実行しない", async () => {
    await withTempDir("install-system-checksum", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const installerMarker = path.join(tempDir, "installer-ran");

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

      const result = await runScript(installLix, fakeBin, {
        SYSTEM_INSTALLER_MARKER: installerMarker,
      });

      expect(result.exitCode).not.toBe(0);
      expect(await Bun.file(installerMarker).exists()).toBe(false);
    });
  });
});
