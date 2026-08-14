import { describe, expect, test } from "bun:test";
import { chmod, copyFile, mkdir, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers";

function runCommand(
  command: string,
  args: string[],
  env: NodeJS.ProcessEnv = process.env,
  options: { cwd?: string; input?: string } = {},
) {
  return new Promise<{ code: number | null; stderr: string; stdout: string }>((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env,
      stdio: [options.input === undefined ? "ignore" : "pipe", "pipe", "pipe"],
    });

    if (options.input !== undefined) {
      child.stdin?.end(options.input);
    }

    let stdout = "";
    let stderr = "";
    child.stdout!.on("data", (chunk) => {
      stdout += String(chunk);
    });
    child.stderr!.on("data", (chunk) => {
      stderr += String(chunk);
    });
    child.on("error", reject);
    child.on("close", (code) => {
      resolve({ code, stderr, stdout });
    });
  });
}

async function makeExecutable(filePath: string, content: string) {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
  await chmod(filePath, 0o755);
}

async function initRepoWithManagedGitConfig(tempDir: string) {
  const homeDir = path.join(tempDir, "home");
  const repoDir = path.join(tempDir, "repo");
  const hooksDir = path.join(homeDir, ".config", "git", "hooks");
  const globalConfigPath = path.join(homeDir, ".gitconfig");
  const gitleaksPath = path.join(homeDir, ".local", "share", "mise", "installs", "gitleaks", "8.30.1", "gitleaks");
  const misePath = path.join(homeDir, ".local", "bin", "mise");
  const mybinDir = path.join(homeDir, "mybin");
  const undoScriptPath = path.join(mybinDir, "git-undo");
  const publicDocumentPrivacyCheckerPath = path.join(hooksDir, "check-public-document-privacy");
  const gitleaksHookPath = path.join(hooksDir, "run-gitleaks-pre-commit");
  const env = {
    ...process.env,
    HOME: homeDir,
    GIT_CONFIG_GLOBAL: globalConfigPath,
    GIT_CONFIG_NOSYSTEM: "1",
  };

  await mkdir(hooksDir, { recursive: true });
  await mkdir(repoDir, { recursive: true });
  await mkdir(mybinDir, { recursive: true });
  await copyFile("home/.gitconfig", globalConfigPath);
  await copyFile("home/mybin/git-undo", undoScriptPath);
  await copyFile("home/.config/git/hooks/check-public-document-privacy", publicDocumentPrivacyCheckerPath);
  await copyFile("home/.config/git/hooks/run-gitleaks-pre-commit", gitleaksHookPath);
  await chmod(undoScriptPath, 0o755);
  await chmod(publicDocumentPrivacyCheckerPath, 0o755);
  await chmod(gitleaksHookPath, 0o755);
  await makeExecutable(
    misePath,
    `#!/bin/sh
[ "$1" = "which" ] && [ "$2" = "gitleaks" ] || exit 2
printf '%s\n' "$HOME/.local/share/mise/installs/gitleaks/8.30.1/gitleaks"
`,
  );
  await makeExecutable(gitleaksPath, "#!/bin/sh\nexit 0\n");

  const initResult = await runCommand("git", ["-C", repoDir, "init", "-b", "master"], env);
  expect(initResult.code).toBe(0);

  return { env, gitleaksPath, misePath, repoDir };
}

async function runGit(repoDir: string, env: NodeJS.ProcessEnv, ...args: string[]) {
  return runCommand("git", ["-C", repoDir, ...args], env);
}

async function expectGitSupportsConfigBasedHooks(env: NodeJS.ProcessEnv = process.env) {
  const versionResult = await runCommand("git", ["--version"], env);
  expect(versionResult.code).toBe(0);

  const version = versionResult.stdout.trim();
  const match = version.match(/^git version (\d+)\.(\d+)\.(\d+)$/);
  if (!match) {
    throw new Error(`unable to parse git version: ${version || versionResult.stderr.trim()}`);
  }

  const [, majorText, minorText] = match;
  const major = Number(majorText);
  const minor = Number(minorText);
  if (major < 2 || (major === 2 && minor < 54)) {
    throw new Error(`config-based git hooks require git 2.54.0 or later, found ${version}`);
  }
}

async function initPlainRepo(tempDir: string) {
  const repoDir = path.join(tempDir, "repo");

  await mkdir(repoDir, { recursive: true });
  expect((await runCommand("git", ["init", "-b", "master"], process.env, { cwd: repoDir })).code).toBe(0);

  return repoDir;
}

async function writeRepoFile(repoDir: string, relativePath: string, content: string) {
  const filePath = path.join(repoDir, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content, "utf8");
}

async function runPublicDocumentPrivacyChecker(repoDir: string, env: NodeJS.ProcessEnv = process.env) {
  const checkerPath = path.join(process.cwd(), "home/.config/git/hooks/check-public-document-privacy");

  return runCommand("/bin/sh", [checkerPath], env, { cwd: repoDir });
}

async function renderPrompt(repoDir: string) {
  const promptPath = path.join(process.cwd(), "home/.zsh.d/prompt.zsh");

  return runCommand("zsh", [
    "-f",
    "-c",
    'source "$1"; cd "$2"; precmd; print -P -- "$PROMPT"',
    "prompt-test",
    promptPath,
    repoDir,
  ]);
}

async function createMinimalZshHome(tempDir: string, options?: { direnvPath?: string | null }) {
  const homeDir = path.join(tempDir, "home");
  const misePath = path.join(homeDir, ".local", "bin", "mise");
  const direnvPath = options?.direnvPath ?? null;

  await writeTree(homeDir, {
    ".zsh.d/prompt.zsh": "export PROMPT_LOADED=1\n",
    ".zsh.d/keybindings.zsh": "export KEYBINDINGS_LOADED=1\n",
    ".zsh.d/functions.zsh": "export FUNCTIONS_LOADED=1\n",
    ".zsh.d/local.zsh": "export LOCAL_LOADED=1\n",
  });
  await writeTree(path.dirname(misePath), {
    mise: `#!/bin/sh
set -eu
case "$1" in
  activate)
    exit 0
    ;;
  which)
    case "$2" in
      direnv)
        if [ -n "${direnvPath ?? ""}" ]; then
          printf '%s\n' "${direnvPath ?? ""}"
          exit 0
        fi
        exit 1
        ;;
      *)
        exit 1
        ;;
    esac
    ;;
  *)
    exit 1
    ;;
esac
`,
  });
  await chmod(misePath, 0o755);

  return { homeDir };
}

async function createFakeBrew(prefix: string, identity: string) {
  const brewPath = path.join(prefix, "bin", "brew");
  await writeTree(path.dirname(brewPath), {
    brew: `#!/bin/sh
set -eu
prefix=${JSON.stringify(prefix)}
printf '%s:%s\n' ${JSON.stringify(identity)} "\${1-}" >> "$BREW_LOG"
case "\${1-}" in
  shellenv)
    printf 'export HOMEBREW_PREFIX="%s";\n' "$prefix"
    printf 'export HOMEBREW_CELLAR="%s/Cellar";\n' "$prefix"
    printf 'export HOMEBREW_REPOSITORY="%s/Library/.homebrew-is-managed-by-nix";\n' "$prefix"
    printf 'export PATH="%s/bin:%s/sbin:$PATH";\n' "$prefix" "$prefix"
    ;;
  --prefix)
    printf '%s\n' "$prefix"
    ;;
  --cellar)
    printf '%s/Cellar\n' "$prefix"
    ;;
  *)
    exit 1
    ;;
esac
`,
  });
  await chmod(brewPath, 0o755);
  return brewPath;
}

const ZINIT_PLUGINS = [
  { name: "momo-lab/zsh-abbrev-alias", dir: "momo-lab---zsh-abbrev-alias", sha: "33fe094da0a70e279e1cc5376a3d7cb7a5343df5" },
  { name: "zsh-users/zsh-syntax-highlighting", dir: "zsh-users---zsh-syntax-highlighting", sha: "1d85c692615a25fe2293bdd44b34c217d5d2bf04" },
  { name: "zsh-users/zsh-autosuggestions", dir: "zsh-users---zsh-autosuggestions", sha: "85919cd1ffa7d2d5412f6d3fe437ebdbeeec4fc5" },
] as const;

async function preparePinnedZinitHome(tempDir: string, options: { revisionOverrides?: Partial<Record<string, string>>; dirty?: string[] } = {}) {
  const fakeBin = path.join(tempDir, "bin");
  const gitLogPath = path.join(tempDir, "zinit-git.log");
  const loadLogPath = path.join(tempDir, "zinit-load.log");
  const { homeDir } = await createMinimalZshHome(tempDir);
  const pluginsDir = path.join(homeDir, ".local", "share", "zinit", "plugins");
  const zinitHome = path.join(homeDir, ".local", "share", "zinit", "zinit.git");

  const configDir = path.join(homeDir, ".config", "mise");
  const configLines = ZINIT_PLUGINS.map(
    (plugin) =>
      `"~/.local/share/zinit/plugins/${plugin.dir}" = { url = "https://github.com/${plugin.name}.git", ref = "${plugin.sha}" }`,
  );
  await writeTree(configDir, { "config.toml": `[bootstrap.repos]\n${configLines.join("\n")}\n` });

  for (const plugin of ZINIT_PLUGINS) {
    const pluginDir = path.join(pluginsDir, plugin.dir);
    await mkdir(path.join(pluginDir, ".git"), { recursive: true });
    await writeFile(path.join(pluginDir, ".test-revision"), `${options.revisionOverrides?.[plugin.dir] ?? plugin.sha}\n`);
  }
  for (const dirty of options.dirty ?? []) {
    await writeFile(path.join(pluginsDir, dirty, ".test-status"), "?? stray.zwc\n");
  }
  await writeTree(zinitHome, {
    "zinit.zsh": `typeset -gA ZINIT
ZINIT[PLUGINS_DIR]="${pluginsDir}"
zinit() {
  if [[ "$1" = light ]]; then
    print -r -- "$2" >> "$ZINIT_LOAD_LOG"
  fi
}
`,
  });
  const gitPath = path.join(fakeBin, "git");
  await writeTree(fakeBin, {
    git: `#!/bin/sh
set -eu
plugin_dir="$2"
shift 2
printf '<%s>' "$@" >> "$ZINIT_GIT_LOG"
printf '\n' >> "$ZINIT_GIT_LOG"
case "$1" in
  rev-parse)
    /bin/cat "$plugin_dir/.test-revision" 2>/dev/null
    ;;
  status)
    /bin/cat "$plugin_dir/.test-status" 2>/dev/null
    ;;
  clone|fetch|checkout|clean)
    exit 1
    ;;
  *) exit 1 ;;
esac
`,
  });
  await chmod(gitPath, 0o755);

  return {
    env: {
      ...process.env,
      DOTFILES_NO_BANNER: "1",
      HOME: homeDir,
      PATH: `${fakeBin}:${path.join(homeDir, ".local", "bin")}:/usr/bin:/bin`,
      XDG_CONFIG_HOME: "",
      ZINIT_GIT_LOG: gitLogPath,
      ZINIT_LOAD_LOG: loadLogPath,
    },
    gitLogPath,
    loadLogPath,
  };
}

describe("シェル設定", () => {
  test("対話シェルの設定より先にzshenvが秘密情報を読み込む", async () => {
    await withTempDir("zshenv-secrets", async (tempDir) => {
      const homeDir = path.join(tempDir, "home");

      await writeTree(homeDir, {
        ".zsh.d/secrets.zsh": "export SECRET_FROM_TEST=loaded\n",
      });

      const result = await runCommand("zsh", ["-f", "-c", "source home/.zshenv; printf '%s' \"$SECRET_FROM_TEST\""], {
        ...process.env,
        HOME: homeDir,
      });

      expect(result.code).toBe(0);
      expect(result.stdout).toBe("loaded");
    });
  });

  test("zshenvはnix-homebrewが選んだHomebrew環境だけを維持する", async () => {
    await withTempDir("zshenv-homebrew", async (tempDir) => {
      const homeDir = path.join(tempDir, "home");
      const activePrefix = path.join(tempDir, "opt", "homebrew");
      const intelPrefix = path.join(tempDir, "usr", "local");
      const linuxPrefix = path.join(tempDir, "home", "linuxbrew", ".linuxbrew");
      const brewLog = path.join(tempDir, "brew.log");
      const activeBrew = await createFakeBrew(activePrefix, "active");
      await createFakeBrew(intelPrefix, "intel");
      await createFakeBrew(linuxPrefix, "linux");

      const result = await runCommand(
        "zsh",
        [
          "-f",
          "-c",
          [
            "PATH=$TEST_BREW_PATH",
            "rehash",
            "source nix/homebrew-shellenv.zsh",
            "command -v brew",
            "print -r -- $HOMEBREW_PREFIX",
            "print -r -- $HOMEBREW_CELLAR",
            "print -r -- $HOMEBREW_REPOSITORY",
            "brew --prefix",
            "brew --cellar",
            "source home/.zshenv",
            "print -r -- $HOMEBREW_PREFIX",
            "print -r -- $HOMEBREW_CELLAR",
            "print -r -- $HOMEBREW_REPOSITORY",
            "print -r -- $path[(i)/run/current-system/sw/bin] $path[(i)$HOME/.local/share/mise/shims] $path[(i)$HOMEBREW_PREFIX/bin]",
          ].join("; "),
        ],
        {
          ...process.env,
          BREW_LOG: brewLog,
          HOME: homeDir,
          TEST_BREW_PATH: [
            path.join(activePrefix, "bin"),
            path.join(intelPrefix, "bin"),
            path.join(linuxPrefix, "bin"),
            "/usr/bin",
            "/bin",
          ].join(":"),
        },
      );

      expect(result.code).toBe(0);
      const output = result.stdout.trim().split("\n");
      expect(output.slice(0, 9)).toEqual([
        activeBrew,
        activePrefix,
        path.join(activePrefix, "Cellar"),
        path.join(activePrefix, "Library", ".homebrew-is-managed-by-nix"),
        activePrefix,
        path.join(activePrefix, "Cellar"),
        activePrefix,
        path.join(activePrefix, "Cellar"),
        path.join(activePrefix, "Library", ".homebrew-is-managed-by-nix"),
      ]);
      const [nixIndex, miseIndex, brewIndex] = output[9].split(" ").map(Number);
      expect(nixIndex).toBeLessThan(brewIndex);
      expect(miseIndex).toBeLessThan(brewIndex);
      expect(await readFile(brewLog, "utf8")).toBe("active:shellenv\nactive:--prefix\nactive:--cellar\n");
    });
  });

  test("zshenvはNixのsystem packageをmacOS標準コマンドより先に解決する", async () => {
    const result = await runCommand("zsh", [
      "-f",
      "-c",
      "source home/.zshenv; printf '%s %s' $path[(i)/run/current-system/sw/bin] $path[(i)/usr/bin]",
    ]);

    expect(result.code).toBe(0);
    const [nixIndex, systemIndex] = result.stdout.split(" ").map(Number);
    expect(nixIndex).toBeLessThan(systemIndex);
  });

  test("非対話シェルでもmiseと管理対象のコマンドを解決する", async () => {
    await withTempDir("zshenv-mise-path", async (tempDir) => {
      const homeDir = path.join(tempDir, "home");
      const misePath = path.join(homeDir, ".local", "bin", "mise");
      const rgPath = path.join(homeDir, ".local", "share", "mise", "shims", "rg");

      await writeTree(path.dirname(misePath), { mise: "#!/bin/sh\nexit 0\n" });
      await writeTree(path.dirname(rgPath), { rg: "#!/bin/sh\nexit 0\n" });
      await chmod(misePath, 0o755);
      await chmod(rgPath, 0o755);

      const result = await runCommand(
        "zsh",
        ["-f", "-c", "source home/.zshenv; command -v mise; command -v rg"],
        { ...process.env, HOME: homeDir, PATH: "/usr/bin:/bin" },
      );

      expect(result.code).toBe(0);
      expect(result.stdout).toBe(`${misePath}\n${rgPath}\n`);
    });
  });

  test("mise有効化後もNixのsystem packageをmacOS標準コマンドより先に解決する", async () => {
    await withTempDir("zshrc-nix-path", async (tempDir) => {
      const { homeDir } = await createMinimalZshHome(tempDir);
      const result = await runCommand(
        "/bin/zsh",
        [
          "-f",
          "-i",
          "-c",
          "source home/.zshenv; source home/.zshrc; printf '%s %s' $path[(i)/run/current-system/sw/bin] $path[(i)/usr/bin]",
        ],
        {
          ...process.env,
          DOTFILES_NO_BANNER: "1",
          HOME: homeDir,
          PATH: `${path.join(homeDir, ".local", "bin")}:/usr/bin:/run/current-system/sw/bin`,
        },
      );

      expect(result.code).toBe(0);
      const [nixIndex, systemIndex] = result.stdout.split(" ").map(Number);
      expect(nixIndex).toBeLessThan(systemIndex);
    });
  });

  test("対話シェルでzshrcが管理対象の設定断片とリポジトリ用エイリアスを読み込む", async () => {
    await withTempDir("zshrc-fragments", async (tempDir) => {
      const { homeDir } = await createMinimalZshHome(tempDir);

      await copyFile("home/.zsh.d/alias.zsh", path.join(homeDir, ".zsh.d", "alias.zsh"));

      const result = await runCommand(
        "zsh",
        [
          "-f",
          "-i",
          "-c",
          "abbrev-alias(){ alias \"$@\"; }; source home/.zshrc; alias codex; printf ' prompt=%s key=%s fn=%s local=%s' \"$PROMPT_LOADED\" \"$KEYBINDINGS_LOADED\" \"$FUNCTIONS_LOADED\" \"$LOCAL_LOADED\"",
        ],
        {
          ...process.env,
          DOTFILES_NO_BANNER: "1",
          HOME: homeDir,
          PATH: `${path.join(homeDir, ".local", "bin")}:${process.env.PATH ?? ""}`,
        },
      );

      expect(result.code).toBe(0);
      expect(result.stdout).toContain("codex='command codex --no-alt-screen'");
      expect(result.stdout).toContain("prompt=1 key=1 fn=1 local=1");
    });
  });

  test("miseがdirenvを解決できる場合だけzshrcがdirenvを有効にする", async () => {
    await withTempDir("zshrc-direnv", async (tempDir) => {
      const binDir = path.join(tempDir, "bin");
      const direnvPath = path.join(binDir, "direnv");
      const { homeDir } = await createMinimalZshHome(tempDir, { direnvPath });

      await writeTree(binDir, {
        direnv: `#!/bin/sh
printf '%s\n' 'export DIRENV_HOOK_LOADED=1'
`,
      });
      await copyFile("home/.zsh.d/alias.zsh", path.join(homeDir, ".zsh.d", "alias.zsh"));
      await chmod(direnvPath, 0o755);

      const result = await runCommand(
        "zsh",
        [
          "-f",
          "-i",
          "-c",
          "abbrev-alias(){ alias \"$@\"; }; source home/.zshrc; printf '%s' \"$DIRENV_HOOK_LOADED\"",
        ],
        {
          ...process.env,
          DOTFILES_NO_BANNER: "1",
          HOME: homeDir,
          PATH: `${path.join(homeDir, ".local", "bin")}:${binDir}:${process.env.PATH ?? ""}`,
        },
      );

      expect(result.code).toBe(0);
      expect(result.stdout).toBe("1");
    });
  });

  test("install:userが取得したZinit pluginを固定commitで読み込む", async () => {
    await withTempDir("zinit-pinned", async (tempDir) => {
      const { env, gitLogPath, loadLogPath } = await preparePinnedZinitHome(tempDir);

      const result = await runCommand("zsh", ["-f", "-i", "-c", "source home/.zshrc"], env);

      expect(result.code).toBe(0);
      expect(await readFile(loadLogPath, "utf8")).toBe(
        "momo-lab/zsh-abbrev-alias\n" +
          "zsh-users/zsh-syntax-highlighting\n" +
          "zsh-users/zsh-autosuggestions\n",
      );
      const gitCalls = (await readFile(gitLogPath, "utf8")).trimEnd().split("\n");
      expect(gitCalls).toHaveLength(6);
      expect(gitCalls).toEqual([
        "<rev-parse><HEAD>",
        "<status><--porcelain>",
        "<rev-parse><HEAD>",
        "<status><--porcelain>",
        "<rev-parse><HEAD>",
        "<status><--porcelain>",
      ]);
    });
  });

  test("Zinit pluginのresolved commitが固定値と違えば読み込まない", async () => {
    await withTempDir("zinit-mismatch", async (tempDir) => {
      const { env, loadLogPath } = await preparePinnedZinitHome(tempDir, {
        revisionOverrides: {
          "zsh-users---zsh-syntax-highlighting": "0000000000000000000000000000000000000000",
        },
      });

      const result = await runCommand("zsh", ["-f", "-i", "-c", "source home/.zshrc"], env);

      expect(result.code).toBe(0);
      expect(result.stderr).toContain("refusing zsh-users/zsh-syntax-highlighting");
      const loaded = await readFile(loadLogPath, "utf8");
      expect(loaded).toContain("momo-lab/zsh-abbrev-alias\n");
      expect(loaded).not.toContain("zsh-users/zsh-syntax-highlighting\n");
      expect(loaded).toContain("zsh-users/zsh-autosuggestions\n");
    });
  });

  test("dirtyなworktreeのZinit pluginは読み込まない", async () => {
    await withTempDir("zinit-dirty", async (tempDir) => {
      const { env, loadLogPath } = await preparePinnedZinitHome(tempDir, {
        dirty: ["zsh-users---zsh-autosuggestions"],
      });

      const result = await runCommand("zsh", ["-f", "-i", "-c", "source home/.zshrc"], env);

      expect(result.code).toBe(0);
      expect(result.stderr).toContain("refusing zsh-users/zsh-autosuggestions");
      const loaded = await readFile(loadLogPath, "utf8");
      expect(loaded).toContain("momo-lab/zsh-abbrev-alias\n");
      expect(loaded).toContain("zsh-users/zsh-syntax-highlighting\n");
      expect(loaded).not.toContain("zsh-users/zsh-autosuggestions\n");
    });
  });

  test("Git branch名のprompt escapeをliteralとして表示する", async () => {
    await withTempDir("prompt-branch-escape", async (tempDir) => {
      const repoDir = await initPlainRepo(tempDir);

      await writeRepoFile(repoDir, "note.txt", "initial\n");
      expect((await runCommand("git", ["add", "note.txt"], process.env, { cwd: repoDir })).code).toBe(0);
      expect(
        (
          await runCommand(
            "git",
            ["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-m", "initial"],
            process.env,
            { cwd: repoDir },
          )
        ).code,
      ).toBe(0);
      expect(
        (await runCommand("git", ["switch", "-c", "topic-%F{red}literal"], process.env, { cwd: repoDir })).code,
      ).toBe(0);

      const result = await renderPrompt(repoDir);

      expect(result.code).toBe(0);
      expect(result.stdout).toContain("[topic-%F{red}literal]");
    });
  });

  test("VCS action値のprompt escapeもliteralへ変換する", async () => {
    const promptPath = path.join(process.cwd(), "home/.zsh.d/prompt.zsh");
    const result = await runCommand("zsh", [
      "-f",
      "-c",
      'source "$1"; typeset -A hook_com; hook_com=(branch "safe" action "merge-%F{blue}literal"); +vi-escape-prompt; print -r -- "$hook_com[action]"',
      "prompt-action-test",
      promptPath,
    ]);

    expect(result.code).toBe(0);
    expect(result.stdout).toBe("merge-%%F{blue}literal\n");
  });

  test("promptの通常branchとstaged・unstaged表示を維持する", async () => {
    await withTempDir("prompt-status", async (tempDir) => {
      const repoDir = await initPlainRepo(tempDir);

      await writeRepoFile(repoDir, "note.txt", "initial\n");
      expect((await runCommand("git", ["add", "note.txt"], process.env, { cwd: repoDir })).code).toBe(0);
      expect(
        (
          await runCommand(
            "git",
            ["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-m", "initial"],
            process.env,
            { cwd: repoDir },
          )
        ).code,
      ).toBe(0);
      await writeRepoFile(repoDir, "note.txt", "staged\n");
      expect((await runCommand("git", ["add", "note.txt"], process.env, { cwd: repoDir })).code).toBe(0);
      await writeRepoFile(repoDir, "note.txt", "unstaged\n");

      const result = await renderPrompt(repoDir);
      const rendered = result.stdout.replace(/\u001b\[[0-9;]*m/g, "");

      expect(result.code).toBe(0);
      expect(rendered).toContain("!+[master]");
    });
  });

  test("Git設定で有効にしたpre-commit hookはmanaged gitleaksがなければコミットを拒否する", async () => {
    await withTempDir("pre-commit-gitleaks-missing", async (tempDir) => {
      const { env, misePath, repoDir } = await initRepoWithManagedGitConfig(tempDir);
      await expectGitSupportsConfigBasedHooks(env);
      await rm(misePath);

      await writeRepoFile(repoDir, "README.md", "safe markdown\n");
      expect((await runGit(repoDir, env, "add", "README.md")).code).toBe(0);

      const commitResult = await runGit(repoDir, env, "commit", "-m", "test");
      const commitOutput = `${commitResult.stdout}${commitResult.stderr}`;

      expect(commitResult.code).toBe(1);
      expect(commitOutput).toContain("gitleaks: managed mise is unavailable");
    });
  });

  test("pre-commit hookはmanaged gitleaksでcleanなstageを検査する", async () => {
    await withTempDir("pre-commit-gitleaks-clean", async (tempDir) => {
      const { env, gitleaksPath, repoDir } = await initRepoWithManagedGitConfig(tempDir);
      const logPath = path.join(tempDir, "gitleaks.log");
      await makeExecutable(
        gitleaksPath,
        `#!/bin/sh
printf '<%s>' "$@" > "$GITLEAKS_LOG"
`,
      );
      await writeRepoFile(repoDir, "README.md", "safe markdown\n");
      expect((await runGit(repoDir, env, "add", "README.md")).code).toBe(0);

      const commitResult = await runGit(repoDir, { ...env, GITLEAKS_LOG: logPath }, "commit", "-m", "test");

      expect(commitResult.code).toBe(0);
      expect(await readFile(logPath, "utf8")).toBe("<protect><--staged><--no-banner>");
    });
  });

  for (const failure of [
    { exitCode: 1, message: "gitleaks: secrets detected. Commit aborted." },
    { exitCode: 2, message: "gitleaks: scan failed with exit status 2. Commit aborted." },
  ]) {
    test(`pre-commit hookはgitleaksのexit ${failure.exitCode}でコミットを拒否する`, async () => {
      await withTempDir(`pre-commit-gitleaks-exit-${failure.exitCode}`, async (tempDir) => {
        const { env, gitleaksPath, repoDir } = await initRepoWithManagedGitConfig(tempDir);
        await makeExecutable(gitleaksPath, `#!/bin/sh\nexit ${failure.exitCode}\n`);
        await writeRepoFile(repoDir, "README.md", "safe markdown\n");
        expect((await runGit(repoDir, env, "add", "README.md")).code).toBe(0);

        const commitResult = await runGit(repoDir, env, "commit", "-m", "test");
        const commitOutput = `${commitResult.stdout}${commitResult.stderr}`;

        expect(commitResult.code).not.toBe(0);
        expect(commitOutput).toContain(failure.message);
      });
    });
  }

  test("公開文書の検査は環境を特定できるパスを含むステージ済みMarkdownを拒否する", async () => {
    await withTempDir("public-document-privacy-checker-leak", async (tempDir) => {
      const repoDir = await initPlainRepo(tempDir);

      await writeRepoFile(repoDir, "README.md", "/Users/example/private/project\n");
      expect((await runCommand("git", ["add", "README.md"], process.env, { cwd: repoDir })).code).toBe(0);

      const checkResult = await runPublicDocumentPrivacyChecker(repoDir);

      expect(checkResult.code).toBe(1);
      expect(checkResult.stderr).toContain("public document privacy issues detected in staged markdown:");
      expect(checkResult.stderr).toContain("README.md:1:/Users/example/private/project");
    });
  });

  test("公開文書の検査は変更していない既存行を再検査しない", async () => {
    await withTempDir("public-document-privacy-checker-existing-leak", async (tempDir) => {
      const repoDir = await initPlainRepo(tempDir);

      await writeRepoFile(repoDir, "README.md", "/Users/example/private/project\n");
      expect((await runCommand("git", ["add", "README.md"], process.env, { cwd: repoDir })).code).toBe(0);
      expect(
        (await runCommand("git", ["commit", "--no-verify", "-m", "initial"], process.env, { cwd: repoDir }))
          .code,
      ).toBe(0);

      await writeRepoFile(repoDir, "README.md", "/Users/example/private/project\nsafe addition\n");
      expect((await runCommand("git", ["add", "README.md"], process.env, { cwd: repoDir })).code).toBe(0);

      const checkResult = await runPublicDocumentPrivacyChecker(repoDir);

      expect(checkResult.code).toBe(0);
      expect(checkResult.stderr).toBe("");
    });
  });

  test("公開文書の検査は単語中のhomeと汎用placeholderを許可する", async () => {
    await withTempDir("public-document-privacy-checker-placeholders", async (tempDir) => {
      const repoDir = await initPlainRepo(tempDir);

      await writeRepoFile(
        repoDir,
        "docs/guide.md",
        "application/home/config\n/Users/.../project\n/home/<user>/project\n",
      );
      expect((await runCommand("git", ["add", "docs/guide.md"], process.env, { cwd: repoDir })).code).toBe(0);

      const checkResult = await runPublicDocumentPrivacyChecker(repoDir);

      expect(checkResult.code).toBe(0);
      expect(checkResult.stderr).toBe("");
    });
  });

  test("公開文書の検査は追加された環境固有パスの行番号を報告する", async () => {
    await withTempDir("public-document-privacy-checker-added-line", async (tempDir) => {
      const repoDir = await initPlainRepo(tempDir);

      await writeRepoFile(repoDir, "README.md", "safe line\n");
      expect((await runCommand("git", ["add", "README.md"], process.env, { cwd: repoDir })).code).toBe(0);
      expect(
        (await runCommand("git", ["commit", "--no-verify", "-m", "initial"], process.env, { cwd: repoDir }))
          .code,
      ).toBe(0);

      await writeRepoFile(repoDir, "README.md", "safe line\nsee /home/example/private/project\n");
      expect((await runCommand("git", ["add", "README.md"], process.env, { cwd: repoDir })).code).toBe(0);

      const checkResult = await runPublicDocumentPrivacyChecker(repoDir);

      expect(checkResult.code).toBe(1);
      expect(checkResult.stderr).toContain("README.md:2:see /home/example/private/project");
    });
  });

  test("公開文書の検査は環境固有のパスを含まないステージ済みMarkdownを許可する", async () => {
    await withTempDir("public-document-privacy-checker-safe", async (tempDir) => {
      const repoDir = await initPlainRepo(tempDir);

      await writeRepoFile(repoDir, "docs/guide.md", "https://example.com/reference\n");
      expect((await runCommand("git", ["add", "docs/guide.md"], process.env, { cwd: repoDir })).code).toBe(0);

      const checkResult = await runPublicDocumentPrivacyChecker(repoDir);

      expect(checkResult.code).toBe(0);
      expect(checkResult.stderr).toBe("");
    });
  });

  test("公開文書の検査はステージ済みのMarkdown以外を無視する", async () => {
    await withTempDir("public-document-privacy-checker-non-markdown", async (tempDir) => {
      const repoDir = await initPlainRepo(tempDir);

      await writeRepoFile(repoDir, "notes.txt", "/Users/example/private/project\n");
      expect((await runCommand("git", ["add", "notes.txt"], process.env, { cwd: repoDir })).code).toBe(0);

      const checkResult = await runPublicDocumentPrivacyChecker(repoDir);

      expect(checkResult.code).toBe(0);
      expect(checkResult.stderr).toBe("");
    });
  });

  test("Git設定で有効にしたpre-commit hookは公開文書の検査に失敗したコミットを拒否する", async () => {
    await withTempDir("pre-commit-public-document-privacy", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);
      await expectGitSupportsConfigBasedHooks(env);

      await writeRepoFile(repoDir, "README.md", "see file:///Users/example/private/project\n");
      expect((await runGit(repoDir, env, "add", "README.md")).code).toBe(0);

      const commitResult = await runGit(repoDir, env, "commit", "-m", "test");
      const commitOutput = `${commitResult.stdout}${commitResult.stderr}`;

      expect(commitResult.code).toBe(1);
      expect(commitOutput).toContain("public document privacy issues detected in staged markdown:");
      expect(commitOutput).toContain("README.md:1:see file:///Users/example/private/project");
    });
  });

  test("Git設定で有効にしたpre-commit hookは公開文書の検査を実行できなければコミットを拒否する", async () => {
    await withTempDir("pre-commit-public-document-privacy-missing", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);
      await expectGitSupportsConfigBasedHooks(env);
      const checkerPath = path.join(env.HOME!, ".config", "git", "hooks", "check-public-document-privacy");

      await rm(checkerPath);
      await writeRepoFile(repoDir, "README.md", "safe markdown\n");
      expect((await runGit(repoDir, env, "add", "README.md")).code).toBe(0);

      const commitResult = await runGit(repoDir, env, "commit", "-m", "test");
      const commitOutput = `${commitResult.stdout}${commitResult.stderr}`;

      expect(commitResult.code).toBe(1);
      expect(commitOutput).toContain("No such file or directory");
      expect(commitOutput).toContain("check-public-document-privacy");
    });
  });

  test("git undoはコミットを戻す前にステージ済みの変更を外す", async () => {
    await withTempDir("git-undo-unstage", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);

      await writeRepoFile(repoDir, "note.txt", "before\n");
      expect((await runGit(repoDir, env, "add", "note.txt")).code).toBe(0);
      expect((await runGit(repoDir, env, "commit", "-m", "init")).code).toBe(0);

      await writeRepoFile(repoDir, "note.txt", "after\n");
      expect((await runGit(repoDir, env, "add", "note.txt")).code).toBe(0);

      const undoResult = await runGit(repoDir, env, "undo");
      expect(undoResult.code).toBe(0);

      const stagedNames = await runGit(repoDir, env, "diff", "--cached", "--name-only");
      const unstagedNames = await runGit(repoDir, env, "diff", "--name-only");
      expect(stagedNames.stdout).toBe("");
      expect(unstagedNames.stdout).toBe("note.txt\n");
    });
  });

  test("git undoは最新コミットをステージ済みの変更へ戻す", async () => {
    await withTempDir("git-undo-commit", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);

      await writeRepoFile(repoDir, "note.txt", "one\n");
      expect((await runGit(repoDir, env, "add", "note.txt")).code).toBe(0);
      expect((await runGit(repoDir, env, "commit", "-m", "init")).code).toBe(0);

      await writeRepoFile(repoDir, "note.txt", "two\n");
      expect((await runGit(repoDir, env, "add", "note.txt")).code).toBe(0);
      expect((await runGit(repoDir, env, "commit", "-m", "second")).code).toBe(0);

      const undoResult = await runGit(repoDir, env, "undo");
      expect(undoResult.code).toBe(0);

      const commitCount = await runGit(repoDir, env, "rev-list", "--count", "HEAD");
      const latestSubject = await runGit(repoDir, env, "log", "-1", "--pretty=%s");
      const stagedNames = await runGit(repoDir, env, "diff", "--cached", "--name-only");
      expect(commitCount.stdout).toBe("1\n");
      expect(latestSubject.stdout).toBe("init\n");
      expect(stagedNames.stdout).toBe("note.txt\n");
    });
  });

  test("git undoは最初のコミットもステージ済みの変更へ戻せる", async () => {
    await withTempDir("git-undo-root", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);

      await writeRepoFile(repoDir, "note.txt", "root\n");
      expect((await runGit(repoDir, env, "add", "note.txt")).code).toBe(0);
      expect((await runGit(repoDir, env, "commit", "-m", "root")).code).toBe(0);

      const undoResult = await runGit(repoDir, env, "undo");
      expect(undoResult.code).toBe(0);

      const headResult = await runGit(repoDir, env, "rev-parse", "--verify", "HEAD");
      const statusResult = await runGit(repoDir, env, "status", "--short");
      expect(headResult.code).not.toBe(0);
      expect(statusResult.stdout).toBe("A  note.txt\n");
    });
  });

  test("git undoにパスを渡すと指定したパスだけをステージから外す", async () => {
    await withTempDir("git-undo-path", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);

      await writeRepoFile(repoDir, "a.txt", "one\n");
      await writeRepoFile(repoDir, "b.txt", "one\n");
      expect((await runGit(repoDir, env, "add", "a.txt", "b.txt")).code).toBe(0);
      expect((await runGit(repoDir, env, "commit", "-m", "init")).code).toBe(0);

      await writeRepoFile(repoDir, "a.txt", "two\n");
      await writeRepoFile(repoDir, "b.txt", "two\n");
      expect((await runGit(repoDir, env, "add", "a.txt", "b.txt")).code).toBe(0);

      const undoResult = await runGit(repoDir, env, "undo", "a.txt");
      expect(undoResult.code).toBe(0);

      const stagedNames = await runGit(repoDir, env, "diff", "--cached", "--name-only");
      const unstagedNames = await runGit(repoDir, env, "diff", "--name-only");
      expect(stagedNames.stdout).toBe("b.txt\n");
      expect(unstagedNames.stdout).toBe("a.txt\n");
    });
  });

  test("git undoに渡したパスがステージされていなければ失敗する", async () => {
    await withTempDir("git-undo-path-error", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);

      await writeRepoFile(repoDir, "note.txt", "one\n");
      expect((await runGit(repoDir, env, "add", "note.txt")).code).toBe(0);
      expect((await runGit(repoDir, env, "commit", "-m", "init")).code).toBe(0);

      await writeRepoFile(repoDir, "note.txt", "two\n");

      const undoResult = await runGit(repoDir, env, "undo", "note.txt");
      expect(undoResult.code).toBe(1);
      expect(undoResult.stderr).toContain("git undo: no staged changes for: note.txt");
    });
  });

  test("git undoは戻すものがなければ失敗する", async () => {
    await withTempDir("git-undo-empty", async (tempDir) => {
      const { env, repoDir } = await initRepoWithManagedGitConfig(tempDir);

      const undoResult = await runGit(repoDir, env, "undo");
      expect(undoResult.code).toBe(1);
      expect(undoResult.stderr).toContain("git undo: nothing to undo");
    });
  });

  test("ASCIIコマンドは隣接するpayloadをそのまま表示する", async () => {
    for (const commandName of ["nonnonbiyori", "renchon"]) {
      const commandPath = path.join("home", "mybin", commandName);
      const payload = await readFile(`${commandPath}.ascii`, "utf8");
      const result = await runCommand(commandPath, []);

      expect(result.code).toBe(0);
      expect(result.stderr).toBe("");
      expect(result.stdout).toBe(payload);
    }
  });

  test("tadaの同梱成果物は署名済みApple Silicon実行ファイルである", async () => {
    const artifactPath = "home/mybin/lib/tada-darwin-arm64";
    const executableResult = await runCommand("test", ["-x", artifactPath]);
    const architectureResult = await runCommand("lipo", ["-archs", artifactPath]);
    const signatureResult = await runCommand("codesign", ["--verify", "--strict", artifactPath]);

    expect(executableResult.code).toBe(0);
    expect(architectureResult.code).toBe(0);
    expect(architectureResult.stdout.trim()).toBe("arm64");
    expect(signatureResult.code).toBe(0);
  });

  test("tadaは未対応の環境で何も表示せず正常終了する", async () => {
    const result = await runCommand("sh", ["home/mybin/tada"], {
      ...process.env,
      TADA_UNAME: "Linux",
    });

    expect(result.code).toBe(0);
    expect(result.stdout).toBe("");
    expect(result.stderr).toBe("");
  });

  test("tadaはmacOSで同梱バイナリを起動する", async () => {
    await withTempDir("tada", async (tempDir) => {
      const binDir = path.join(tempDir, "bin");
      const launchCapturePath = path.join(tempDir, "launched");
      const bundledBinaryPath = path.join(tempDir, "tada-darwin-arm64");

      await writeTree(binDir, {
        nohup: `#!/bin/sh
"$@"
`,
      });

      await writeTree(tempDir, {
        "tada-darwin-arm64": `#!/bin/sh
printf 'ok\n' > "${launchCapturePath}"
`,
      });

      await Promise.all([
        chmod(path.join(binDir, "nohup"), 0o755),
        chmod(bundledBinaryPath, 0o755),
      ]);

      const result = await runCommand("sh", ["home/mybin/tada"], {
        ...process.env,
        PATH: `${binDir}:${process.env.PATH ?? ""}`,
        TADA_NOHUP_BIN: path.join(binDir, "nohup"),
        TADA_BIN_PATH: bundledBinaryPath,
        TADA_UNAME: "Darwin",
      });

      expect(result.code).toBe(0);
      for (let attempts = 0; attempts < 50; attempts += 1) {
        try {
          expect(await readFile(launchCapturePath, "utf8")).toContain("ok");
          return;
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
            throw error;
          }
          await Bun.sleep(20);
        }
      }
      expect(await readFile(launchCapturePath, "utf8")).toContain("ok");
    });
  });

  test("tadaはシンボリックリンク経由で起動しても実体から同梱バイナリを探す", async () => {
    await withTempDir("tada", async (tempDir) => {
      const binDir = path.join(tempDir, "bin");
      const realLauncherDir = path.join(tempDir, "repo", "home", "mybin");
      const symlinkLauncherDir = path.join(tempDir, "home", "mybin");
      const launchCapturePath = path.join(tempDir, "launched");
      const realLauncherPath = path.join(realLauncherDir, "tada");
      const symlinkLauncherPath = path.join(symlinkLauncherDir, "tada");
      const bundledBinaryPath = path.join(realLauncherDir, "lib", "tada-darwin-arm64");

      await writeTree(binDir, {
        nohup: `#!/bin/sh
"$@"
`,
      });
      await Promise.all([
        mkdir(path.join(realLauncherDir, "lib"), { recursive: true }),
        mkdir(symlinkLauncherDir, { recursive: true }),
      ]);
      await copyFile("home/mybin/tada", realLauncherPath);
      await writeTree(realLauncherDir, {
        "lib/tada-darwin-arm64": `#!/bin/sh
printf 'ok\n' > "${launchCapturePath}"
`,
      });
      await symlink(realLauncherPath, symlinkLauncherPath);
      await Promise.all([
        chmod(path.join(binDir, "nohup"), 0o755),
        chmod(realLauncherPath, 0o755),
        chmod(bundledBinaryPath, 0o755),
      ]);

      const result = await runCommand("sh", [symlinkLauncherPath], {
        ...process.env,
        PATH: `${binDir}:${process.env.PATH ?? ""}`,
        TADA_NOHUP_BIN: path.join(binDir, "nohup"),
        TADA_UNAME: "Darwin",
      });

      expect(result.code).toBe(0);
      for (let attempts = 0; attempts < 50; attempts += 1) {
        try {
          expect(await readFile(launchCapturePath, "utf8")).toContain("ok");
          return;
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
            throw error;
          }
          await Bun.sleep(20);
        }
      }
      expect(await readFile(launchCapturePath, "utf8")).toContain("ok");
    });
  });

  test("timerは記録なしで終了操作と経過時間表示を維持する", async () => {
    await withTempDir("timer-plain", async (tempDir) => {
      const timerPath = path.resolve("home/mybin/timer");
      const result = await runCommand("ruby", [timerPath], process.env, {
        cwd: tempDir,
        input: "q",
      });

      expect(result.code).toBe(0);
      expect(result.stdout).toContain("quit : 'q'");
      expect(result.stdout).toContain("\u001b[32m00:00:00\u001b[0m");
      expect(result.stdout).not.toContain("save :");
      expect(await readdir(tempDir)).toEqual([]);
    });
  });

  test("timerは存在しないlogを0として作成し、記録と合計を表示する", async () => {
    await withTempDir("timer-recorded", async (tempDir) => {
      const timerPath = path.resolve("home/mybin/timer");
      const logPath = path.join(tempDir, "work.log");
      const result = await runCommand("ruby", [timerPath, logPath], process.env, { input: "q" });
      const logLines = (await readFile(logPath, "utf8")).trimEnd().split("\n");

      expect(result.code).toBe(0);
      expect(result.stdout).toContain("total: 00:00:00");
      expect(result.stdout).toContain(`save : ${logPath}`);
      expect(logLines).toHaveLength(3);
      expect(logLines[2]).toBe("00:00:00");
    });
  });

  test("timerは既存記録の合計へ新しい記録を追記する", async () => {
    await withTempDir("timer-total", async (tempDir) => {
      const timerPath = path.resolve("home/mybin/timer");
      const logPath = path.join(tempDir, "work.log");
      await writeFile(logPath, "2026-08-03 10:00:00 +0900\n2026-08-03 10:00:01 +0900\n00:00:01\n");

      const result = await runCommand("ruby", [timerPath, logPath], process.env, { input: "q" });
      const logLines = (await readFile(logPath, "utf8")).trimEnd().split("\n");

      expect(result.code).toBe(0);
      expect(result.stdout.match(/total: 00:00:01/g)).toHaveLength(2);
      expect(logLines).toHaveLength(6);
    });
  });

  test("timerは破損logと読めないlogを異なる失敗として扱う", async () => {
    await withTempDir("timer-errors", async (tempDir) => {
      const timerPath = path.resolve("home/mybin/timer");
      const corruptLogPath = path.join(tempDir, "corrupt.log");
      const unreadableLogPath = path.join(tempDir, "unreadable.log");
      await writeFile(corruptLogPath, "broken\n");
      await writeFile(unreadableLogPath, "2026-08-03 10:00:00 +0900\n2026-08-03 10:00:01 +0900\n00:00:01\n");
      await chmod(unreadableLogPath, 0o000);

      const corruptResult = await runCommand("ruby", [timerPath, corruptLogPath], process.env, { input: "q" });
      const unreadableResult = await runCommand("ruby", [timerPath, unreadableLogPath], process.env, { input: "q" });

      expect(corruptResult.code).toBe(1);
      expect(corruptResult.stderr).toContain("timer: corrupt log");
      expect(unreadableResult.code).toBe(1);
      expect(unreadableResult.stderr).toContain("timer: cannot read log");
    });
  });

});