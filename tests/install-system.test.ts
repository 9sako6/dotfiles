import { describe, expect, test } from "bun:test";
import { chmod, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir } from "./test-helpers";

const repoRoot = path.resolve(import.meta.dir, "..");
const installLix = path.join(repoRoot, "bin/install-lix.sh");
const installMise = path.join(repoRoot, "bin/install-mise.sh");
const installSystemLibrary = path.join(
  repoRoot,
  "lib/install-system.sh",
);

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

async function prepareMiseInstaller(
  tempDir: string,
  fakeBin: string,
  installedVersion = "2026.7.7",
) {
  const installerPath = path.join(tempDir, "mise-installer.sh");

  await makeExecutable(
    installerPath,
    `#!/bin/sh
set -eu
mkdir -p "$(dirname "$MISE_INSTALL_PATH")"
printf '%s\n' '#!/bin/sh' 'printf "%s\\n" "${installedVersion} macos-arm64"' > "$MISE_INSTALL_PATH"
chmod u+x "$MISE_INSTALL_PATH"
printf '%s\n' "$MISE_VERSION" > "$MISE_INSTALL_MARKER"
`,
  );
  await makeExecutable(
    path.join(fakeBin, "curl"),
    `#!/bin/sh
set -eu
output=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output)
      output="$2"
      shift 2
      ;;
    *) shift ;;
  esac
done
/bin/cp "$MISE_FAKE_INSTALLER" "$output"
`,
  );
  await makeExecutable(
    path.join(fakeBin, "shasum"),
    `#!/bin/sh
/bin/cat > "$MISE_CHECKSUM_LOG"
exit "\${MISE_SHASUM_EXIT:-0}"
`,
  );

  return installerPath;
}

async function runInstallSystemFunction(
  command: string,
  args: string[],
  env: Record<string, string> = {},
) {
  const proc = Bun.spawn(
    [
      "/bin/sh",
      "-c",
      `. "$1"
shift
${command}`,
      "install-system-test",
      installSystemLibrary,
      ...args,
    ],
    {
      cwd: repoRoot,
      env: { ...process.env, ...env },
      stderr: "pipe",
      stdout: "pipe",
    },
  );

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
      const checksumLog = path.join(tempDir, "checksum.log");
      const tempRoot = path.join(tempDir, "tmp");

      await mkdir(tempRoot);
      const installerPath = await prepareMiseInstaller(tempDir, fakeBin);

      const result = await runScript(installMise, fakeBin, {
        HOME: tempDir,
        MISE_CHECKSUM_LOG: checksumLog,
        MISE_FAKE_INSTALLER: installerPath,
        MISE_INSTALL_MARKER: installMarker,
        TMPDIR: tempRoot,
      });

      expect(result).toMatchObject({ exitCode: 0, stderr: "", stdout: "" });
      expect(await readFile(installMarker, "utf8")).toBe("v2026.7.7\n");
      expect(await readFile(checksumLog, "utf8")).toMatch(
        /^0b98c2dc48edc807be860a76e14209afcfe36684c591f92337c5d9ff909e7740  .*\/install\.sh\n$/,
      );
      expect(await readdir(tempRoot)).toEqual([]);
    });
  });

  test("ダウンロードに失敗したら一時ファイルを残さない", async () => {
    await withTempDir("install-mise-download-failure", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const tempRoot = path.join(tempDir, "tmp");

      await mkdir(tempRoot);
      await makeExecutable(path.join(fakeBin, "curl"), "#!/bin/sh\nexit 22\n");

      const result = await runScript(installMise, fakeBin, {
        HOME: tempDir,
        TMPDIR: tempRoot,
      });

      expect(result.exitCode).not.toBe(0);
      expect(await readdir(tempRoot)).toEqual([]);
    });
  });

  test("チェックサムが一致しないinstallerを実行しない", async () => {
    await withTempDir("install-mise-integrity-failure", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const installMarker = path.join(tempDir, "installed-version");
      const checksumLog = path.join(tempDir, "checksum.log");
      const tempRoot = path.join(tempDir, "tmp");

      await mkdir(tempRoot);
      const installerPath = await prepareMiseInstaller(tempDir, fakeBin);
      const result = await runScript(installMise, fakeBin, {
        HOME: tempDir,
        MISE_CHECKSUM_LOG: checksumLog,
        MISE_FAKE_INSTALLER: installerPath,
        MISE_INSTALL_MARKER: installMarker,
        MISE_SHASUM_EXIT: "1",
        TMPDIR: tempRoot,
      });

      expect(result.exitCode).not.toBe(0);
      expect(await Bun.file(installMarker).exists()).toBe(false);
      expect(await readdir(tempRoot)).toEqual([]);
    });
  });

  test("導入された実体のバージョンが違えば失敗して後始末する", async () => {
    await withTempDir("install-mise-version-mismatch", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const installMarker = path.join(tempDir, "installed-version");
      const checksumLog = path.join(tempDir, "checksum.log");
      const tempRoot = path.join(tempDir, "tmp");

      await mkdir(tempRoot);
      const installerPath = await prepareMiseInstaller(tempDir, fakeBin, "2026.7.6");
      const result = await runScript(installMise, fakeBin, {
        HOME: tempDir,
        MISE_CHECKSUM_LOG: checksumLog,
        MISE_FAKE_INSTALLER: installerPath,
        MISE_INSTALL_MARKER: installMarker,
        TMPDIR: tempRoot,
      });

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("expected mise 2026.7.7, got: 2026.7.6");
      expect(await readdir(tempRoot)).toEqual([]);
    });
  });
});

describe("install:system", () => {
  test("非activation buildでも共通のplatformとprimary userを使う", async () => {
    await withTempDir("build-system", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const logPath = path.join(tempDir, "system.log");
      await makeExecutable(
        path.join(fakeBin, "nix"),
        `#!/bin/sh
{
  printf 'primary_user=%s\n' "$DARWIN_PRIMARY_USER"
  printf 'args='
  printf '<%s>' "$@"
  printf '\n'
} > "$SYSTEM_INSTALL_LOG"
`,
      );

      const result = await runInstallSystemFunction(
        'install_system_run_darwin_build "$1" "$2" "$3" "$4"',
        [
          path.join(fakeBin, "nix"),
          "test-user",
          path.join(repoRoot, "darwin"),
          "aarch64-darwin",
        ],
        { SYSTEM_INSTALL_LOG: logPath },
      );

      expect(result).toMatchObject({ exitCode: 0, stderr: "", stdout: "" });
      const log = await readFile(logPath, "utf8");
      expect(log).toContain("primary_user=test-user");
      expect(log).toContain("<--extra-experimental-features><nix-command flakes>");
      expect(log).toContain("<build><--impure><--no-link>");
      expect(log).toMatch(
        /<path:[^>]+\/darwin#darwinConfigurations\.aarch64-darwin\.system>/,
      );
    });
  });

  test("実行時に取得したユーザー名を nix-darwin へ渡す", async () => {
    await withTempDir("install-system", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const logPath = path.join(tempDir, "system.log");

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

      const result = await runInstallSystemFunction(
        'install_system_run_darwin_rebuild "$1" "$2" "$3" "$4" "$5" "$6"',
        [
          path.join(fakeBin, "sudo"),
          "/usr/bin/env",
          path.join(fakeBin, "nix"),
          "test-user",
          path.join(repoRoot, "darwin"),
          "aarch64-darwin",
        ],
        { SYSTEM_INSTALL_LOG: logPath },
      );

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

  test("PATH 上の偽 nix と sudo を特権コマンドとして選ばない", async () => {
    await withTempDir("install-system-path", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const marker = path.join(tempDir, "executed");

      for (const executable of ["nix", "sudo"]) {
        await makeExecutable(
          path.join(fakeBin, executable),
          `#!/bin/sh
touch "${marker}"
`,
        );
      }

      const result = await runInstallSystemFunction(
        `printf 'sudo=%s\\n' "$(install_system_resolve_sudo)"
if nix_bin="$(install_system_resolve_nix)"; then
  printf 'nix=%s\\n' "$nix_bin"
else
  printf 'nix_status=%s\\n' "$?"
fi`,
        [],
        { PATH: `${fakeBin}:/usr/bin:/bin` },
      );

      expect(result.exitCode).toBe(0);
      expect(result.stderr).toBe("");
      expect(result.stdout).toContain("sudo=/usr/bin/sudo\n");
      expect(result.stdout).not.toContain(fakeBin);
      expect(await Bun.file(marker).exists()).toBe(false);
    });
  });

  test("user 所有の実行ファイルを trusted と判定しない", async () => {
    await withTempDir("install-system-owner", async (tempDir) => {
      const executable = path.join(tempDir, "nix");
      await makeExecutable(executable, "#!/bin/sh\n");

      const result = await runInstallSystemFunction(
        'install_system_is_root_owned_readonly "$1"',
        [executable],
      );

      expect(result.exitCode).not.toBe(0);
    });
  });

  test("root 所有でも書き込み可能な path を trusted と判定しない", async () => {
    const result = await runInstallSystemFunction(
      'install_system_is_root_owned_readonly "$1"',
      ["/tmp"],
    );

    expect(result.exitCode).not.toBe(0);
  });

  test("期待する Lix が導入済みなら installer を実行しない", async () => {
    await withTempDir("install-system-lix-present", async (tempDir) => {
      const installer = path.join(tempDir, "install-lix");
      const marker = path.join(tempDir, "installer-ran");
      const nixBin = path.join(tempDir, "nix");
      await makeExecutable(installer, `#!/bin/sh\ntouch "${marker}"\n`);
      await makeExecutable(nixBin, "#!/bin/sh\nprintf '%s\\n' 'nix (Lix, like Nix) 2.95.2'\n");

      const result = await runInstallSystemFunction(
        `nix_fixture="$2"
install_system_resolve_nix() { printf '%s\\n' "$nix_fixture"; }
install_system_ensure_lix "$1"`,
        [installer, nixBin],
      );

      expect(result).toMatchObject({
        exitCode: 0,
        stderr: "",
        stdout: `${nixBin}\n`,
      });
      expect(await Bun.file(marker).exists()).toBe(false);
    });
  });

  test("非 Lix の Nix が導入済みなら拒否する", async () => {
    await withTempDir("install-system-non-lix", async (tempDir) => {
      const installer = path.join(tempDir, "install-lix");
      const marker = path.join(tempDir, "installer-ran");
      const nixBin = path.join(tempDir, "nix");
      await makeExecutable(installer, `#!/bin/sh\ntouch "${marker}"\n`);
      await makeExecutable(nixBin, "#!/bin/sh\nprintf '%s\\n' 'nix (Nix) 2.31.0'\n");

      const result = await runInstallSystemFunction(
        `nix_fixture="$2"
install_system_resolve_nix() { printf '%s\\n' "$nix_fixture"; }
install_system_ensure_lix "$1"`,
        [installer, nixBin],
      );

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("system configuration requires a working Lix installation");
      expect(await Bun.file(marker).exists()).toBe(false);
    });
  });

  test("Lix 未導入なら installer 後に identity を再検証する", async () => {
    await withTempDir("install-system-lix-missing", async (tempDir) => {
      const installer = path.join(tempDir, "install-lix");
      const marker = path.join(tempDir, "installer-ran");
      const nixBin = path.join(tempDir, "nix");
      await makeExecutable(installer, `#!/bin/sh\ntouch "${marker}"\n`);
      await makeExecutable(nixBin, "#!/bin/sh\nprintf '%s\\n' 'nix (Lix, like Nix) 2.95.2'\n");

      const result = await runInstallSystemFunction(
        `install_marker="$2"
nix_fixture="$3"
install_system_resolve_nix() {
  if [ -e "$install_marker" ]; then
    printf '%s\\n' "$nix_fixture"
  else
    return 1
  fi
}
install_system_ensure_lix "$1"`,
        [installer, marker, nixBin],
      );

      expect(result).toMatchObject({
        exitCode: 0,
        stderr: "",
        stdout: `${nixBin}\n`,
      });
      expect(await Bun.file(marker).exists()).toBe(true);
    });
  });

  test("installer 後の実体が Lix でなければ拒否する", async () => {
    await withTempDir("install-system-lix-mismatch", async (tempDir) => {
      const installer = path.join(tempDir, "install-lix");
      const marker = path.join(tempDir, "installer-ran");
      const nixBin = path.join(tempDir, "nix");
      await makeExecutable(installer, `#!/bin/sh\ntouch "${marker}"\n`);
      await makeExecutable(nixBin, "#!/bin/sh\nprintf '%s\\n' 'nix (Nix) 2.31.0'\n");

      const result = await runInstallSystemFunction(
        `install_marker="$2"
nix_fixture="$3"
install_system_resolve_nix() {
  if [ -e "$install_marker" ]; then
    printf '%s\\n' "$nix_fixture"
  else
    return 1
  fi
}
install_system_ensure_lix "$1"`,
        [installer, marker, nixBin],
      );

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("system configuration requires a working Lix installation");
      expect(await Bun.file(marker).exists()).toBe(true);
    });
  });

  test("非対応の CPU ではダウンロード前に終了する", async () => {
    await withTempDir("install-system-arch", async (tempDir) => {
      const fakeBin = path.join(tempDir, "bin");
      const downloadMarker = path.join(tempDir, "downloaded");

      await makeExecutable(
        path.join(fakeBin, "curl"),
        `#!/bin/sh
touch "$SYSTEM_DOWNLOAD_MARKER"
`,
      );

      const result = await runInstallSystemFunction(
        'install_system_host_platform "$1" "$2"',
        ["Darwin", "x86_64"],
        { SYSTEM_DOWNLOAD_MARKER: downloadMarker },
      );

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
