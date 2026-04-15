import { chmod, mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { describe, expect, test } from "bun:test";
import { spawn } from "node:child_process";
import path from "node:path";
import { withTempDir } from "./test-helpers";

function runCommand(
  command: string,
  args: string[],
  env: NodeJS.ProcessEnv = process.env,
  options: { cwd?: string } = {},
) {
  return new Promise<{ code: number | null; stderr: string; stdout: string }>((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += String(chunk);
    });
    child.stderr.on("data", (chunk) => {
      stderr += String(chunk);
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      resolve({ code, stderr, stdout });
    });
  });
}

describe("bootstrap behavior", () => {
  test("install.sh trusts the repo, runs setup, then refreshes global tools", async () => {
    await withTempDir("install-script", async (tempDir) => {
      const homeDir = path.join(tempDir, "home");
      const repoDir = path.join(tempDir, "dotfiles");
      const commandLogPath = path.join(tempDir, "mise-commands.log");
      const misePath = path.join(homeDir, ".local", "bin", "mise");

      await mkdir(path.join(repoDir, ".git"), { recursive: true });
      await mkdir(path.dirname(misePath), { recursive: true });
      await writeFile(
        misePath,
        `#!/bin/sh
printf '%s\n' "$*" >> "${commandLogPath}"
`,
      );
      await chmod(misePath, 0o755);

      const result = await runCommand("sh", ["install.sh"], {
        ...process.env,
        DOTFILES_DIR: repoDir,
        HOME: homeDir,
      }, { cwd: process.cwd() });

      expect(result.code).toBe(0);
      expect(result.stderr).toBe("");
      expect(await readFile(commandLogPath, "utf8")).toBe("trust\ninstall\nrun setup\ninstall\n");
    });
  });

  test("install.sh executes the mise installer from a temp file and removes it afterwards", async () => {
    await withTempDir("install-script-bootstrap", async (tempDir) => {
      const homeDir = path.join(tempDir, "home");
      const repoDir = path.join(tempDir, "dotfiles");
      const binDir = path.join(tempDir, "bin");
      const commandLogPath = path.join(tempDir, "mise-commands.log");
      const executedInstallerPath = path.join(tempDir, "executed-installer-path");

      await Promise.all([
        mkdir(path.join(repoDir, ".git"), { recursive: true }),
        mkdir(binDir, { recursive: true }),
      ]);

      await writeFile(
        path.join(binDir, "curl"),
        `#!/bin/sh
set -eu
cat <<'EOF'
#!/bin/sh
set -eu
mkdir -p "$HOME/.local/bin"
printf '%s\\n' "installer-ran" >> "${commandLogPath}"
cat > "$HOME/.local/bin/mise" <<'EOS'
#!/bin/sh
printf '%s\\n' "$*" >> "${commandLogPath}"
EOS
chmod 755 "$HOME/.local/bin/mise"
EOF
`,
      );
      await writeFile(
        path.join(binDir, "sh"),
        `#!/bin/sh
set -eu
if [ "$#" -eq 0 ]; then
  echo "stdin install is not allowed" >&2
  exit 91
fi
printf '%s' "$1" > "${executedInstallerPath}"
/bin/sh "$@"
`,
      );
      await Promise.all([
        chmod(path.join(binDir, "curl"), 0o755),
        chmod(path.join(binDir, "sh"), 0o755),
      ]);

      const result = await runCommand(
        "/bin/sh",
        ["install.sh"],
        {
          ...process.env,
          DOTFILES_DIR: repoDir,
          HOME: homeDir,
          PATH: `${binDir}:${process.env.PATH ?? ""}`,
        },
        { cwd: process.cwd() },
      );

      const installerPath = await readFile(executedInstallerPath, "utf8");

      expect(result.code).toBe(0);
      expect(result.stderr).toBe("");
      expect(await readFile(commandLogPath, "utf8")).toBe("installer-ran\ntrust\ninstall\nrun setup\ninstall\n");
      expect(installerPath).not.toBe("");
      expect(await stat(installerPath, { throwIfNoEntry: false })).toBeUndefined();
    });
  });
});
