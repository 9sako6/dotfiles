import { describe, expect, test } from "bun:test";
import { chmod, readFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers";

function runCommand(
  command: string,
  args: string[],
  env: NodeJS.ProcessEnv = process.env,
) {
  return new Promise<{ code: number | null; stderr: string; stdout: string }>((resolve, reject) => {
    const child = spawn(command, args, {
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

describe("shell config", () => {
  test("repo-local mise tools stay minimal", async () => {
    const miseToml = await readFile(".mise.toml", "utf8");

    expect(miseToml).toContain('bun = "1.3.11"');
    expect(miseToml).not.toContain('direnv = "latest"');
    expect(miseToml).not.toContain('ghq = "latest"');
    expect(miseToml).not.toContain('fzf = "latest"');
    expect(miseToml).not.toContain('\ngo = "latest"\n');
  });

  test("managed global mise config contains user-wide tools", async () => {
    const globalMiseConfig = await readFile("dist/.config/mise/config.toml", "utf8");

    expect(globalMiseConfig).toContain('atuin = ');
    expect(globalMiseConfig).toContain('bat = ');
    expect(globalMiseConfig).toContain('delta = ');
    expect(globalMiseConfig).toContain('direnv = "2.37.1"');
    expect(globalMiseConfig).toContain('eza = ');
    expect(globalMiseConfig).toContain('fd = ');
    expect(globalMiseConfig).toContain('ghq = "1.9.4"');
    expect(globalMiseConfig).toContain('fzf = "0.70.0"');
    expect(globalMiseConfig).toContain('ripgrep = "15.1.0"');
    expect(globalMiseConfig).toContain('zoxide = ');
    expect(globalMiseConfig).toContain('zellij = "0.44.0"');
    expect(globalMiseConfig).not.toContain('\ngh = ');
  });

  test("zshenv removes stale machine-specific tool bootstrapping", async () => {
    const zshenv = await readFile("dist/.zshenv", "utf8");

    expect(zshenv).not.toContain("go env GOPATH");
    expect(zshenv).not.toContain(".rbenv");
    expect(zshenv).not.toContain("NVM_DIR");
    expect(zshenv).not.toContain(".opam");
    expect(zshenv).not.toContain("kubectl completion zsh");
    expect(zshenv).not.toContain("KREW_ROOT");
    expect(zshenv).not.toContain("/Users/9sako6/");
  });

  test("zshenv sets up Homebrew on macOS as well as Linuxbrew", async () => {
    const zshenv = await readFile("dist/.zshenv", "utf8");

    expect(zshenv).toContain("[ -f '/opt/homebrew/bin/brew' ]");
    expect(zshenv).toContain("eval \"$(/opt/homebrew/bin/brew shellenv)\"");
    expect(zshenv).toContain("[ -f '/home/linuxbrew/.linuxbrew/bin/brew' ]");
  });

  test("zshrc guards direnv hook behind a resolvable mise tool", async () => {
    const zshrc = await readFile("dist/.zshrc", "utf8");

    expect(zshrc).toContain("[[ -o interactive ]] || return");
    expect(zshrc).toContain('eval "$(zoxide init zsh)"');
    expect(zshrc).toContain('eval "$(atuin init zsh');
    expect(zshrc).toContain("if mise which direnv > /dev/null 2>&1; then");
    expect(zshrc).toContain('eval "$(direnv hook zsh)"');
    expect(zshrc).not.toContain("google-cloud-sdk");
    expect(zshrc).not.toContain(".dart-cli-completion");
    expect(zshrc).not.toContain(".antigravity");
    expect(zshrc).not.toContain("PNPM_HOME");
  });

  test("zshrc only loads zinit when the installed file exists", async () => {
    const zshrc = await readFile("dist/.zshrc", "utf8");

    expect(zshrc).toContain('if [ -f "${ZINIT_HOME}/zinit.zsh" ]; then');
    expect(zshrc).not.toContain("git clone https://github.com/zdharma-continuum/zinit.git");
    expect(zshrc).toContain("autoload -U +X compinit && compinit");
    expect(zshrc).not.toContain("fast-syntax-highlighting");
  });

  test("zshrc loads tracked and untracked zsh fragments from .zsh.d", async () => {
    const zshrc = await readFile("dist/.zshrc", "utf8");

    expect(zshrc).toContain('${HOME}/.zsh.d/prompt.zsh');
    expect(zshrc).toContain('${HOME}/.zsh.d/alias.zsh');
    expect(zshrc).toContain('${HOME}/.zsh.d/keybindings.zsh');
    expect(zshrc).toContain('${HOME}/.zsh.d/functions.zsh');
    expect(zshrc).toContain('${HOME}/.zsh.d/local.zsh');
    expect(zshrc).toContain('${HOME}/.zsh.d/secrets.zsh');
    expect(zshrc).not.toContain(".zsh.local");
  });

  test("zshrc makes banner output opt-out and avoids CI noise", async () => {
    const zshrc = await readFile("dist/.zshrc", "utf8");

    expect(zshrc).toContain("DOTFILES_NO_BANNER");
    expect(zshrc).toContain("CI");
    expect(zshrc).toContain("nonnonbiyori");
    expect(zshrc).toContain("renchon");
  });

  test("git config does not depend on GitHub CLI credential helpers", async () => {
    const gitconfig = await readFile("dist/.gitconfig", "utf8");

    expect(gitconfig).not.toContain("gh auth git-credential");
    expect(gitconfig).not.toContain('[credential "https://github.com"]');
    expect(gitconfig).not.toContain('[credential "https://gist.github.com"]');
    expect(gitconfig).toContain("pager = delta");
    expect(gitconfig).toContain('diffFilter = delta --color-only');
  });

  test("shell aliases keep only currently supported tools", async () => {
    const aliases = await readFile("dist/alias.sh", "utf8");
    const abbrevAliases = await readFile("dist/.zsh.d/alias.zsh", "utf8");
    const prompt = await readFile("dist/.zsh.d/prompt.zsh", "utf8");

    expect(aliases).not.toContain('alias tree=');
    expect(aliases).not.toContain("alias vi=nvim");
    expect(aliases).not.toContain('alias view="nvim -R"');
    expect(aliases).not.toContain("alias snowsql=");

    expect(abbrevAliases).not.toContain('abbrev-alias dc="docker-compose"');
    expect(abbrevAliases).not.toContain('abbrev-alias zoit=');
    expect(abbrevAliases).not.toContain('abbrev-alias zoic=');
    expect(abbrevAliases).not.toContain('abbrev-alias k="kubectl"');
    expect(abbrevAliases).not.toContain("intellij-idea-community");
    expect(abbrevAliases).not.toContain("cargo -vv watch");
    expect(abbrevAliases).not.toContain("snowsql");

    expect(prompt).not.toContain("kube-ps1");
    expect(prompt).not.toContain("kube_ps1");
  });

  test("helper scripts avoid dead hardcoded paths", async () => {
    const gppr = await readFile("dist/mybin/gppr", "utf8");
    const diffcop = await readFile("dist/mybin/diffcop", "utf8");

    expect(gppr).not.toContain("/usr/local/bin/g++");
    expect(diffcop).not.toContain("function diffcop");
    expect(diffcop).toContain("rubocop");
  });

  test("tada stays quiet and exits successfully on unsupported environments", async () => {
    const result = await runCommand("sh", ["dist/mybin/tada"], {
      ...process.env,
      TADA_UNAME: "Linux",
    });

    expect(result.code).toBe(0);
    expect(result.stdout).toBe("");
    expect(result.stderr).toBe("");
  });

  test("tada launches asynchronously on macOS via a detached helper", async () => {
    const tada = await readFile("dist/mybin/tada", "utf8");

    expect(tada).toContain("nohup");
    expect(tada).toContain("swift");
    expect(tada).toContain(" >/dev/null 2>&1 &");
    expect(tada).toContain("TADA_UNAME");
  });

  test("tada targets all displays with a full-width lower burst", async () => {
    const tada = await readFile("dist/mybin/tada", "utf8");

    expect(tada).toContain("NSScreen.screens");
    expect(tada).toContain("for screen in NSScreen.screens");
    expect(tada).toContain("height: screenFrame.height");
    expect(tada).toContain("width: screenFrame.width");
    expect(tada).toContain("emitterPosition = CGPoint(x: width / 2, y:");
    expect(tada).toContain("emitterSize = CGSize(width: width");
  });

  test("tada pushes confetti high before it falls across the screen", async () => {
    const tada = await readFile("dist/mybin/tada", "utf8");

    expect(tada).toContain("velocity = 420");
    expect(tada).toContain("velocityRange = 180");
    expect(tada).toContain("yAcceleration = 380");
    expect(tada).toContain("emissionRange = .pi / 1.8");
  });

  test("tada reaches the Swift launcher path on macOS when temp script creation succeeds", async () => {
    await withTempDir("tada", async (tempDir) => {
      const binDir = path.join(tempDir, "bin");
      const scriptCapturePath = path.join(tempDir, "captured.swift");
      const tempScriptPath = path.join(tempDir, "tada-script");

      await writeTree(binDir, {
        mktemp: `#!/bin/sh
printf '%s\n' "${tempScriptPath}"
: > "${tempScriptPath}"
`,
        nohup: `#!/bin/sh
"$@"
`,
        swift: `#!/bin/sh
cp "$1" "${scriptCapturePath}"
`,
      });

      await Promise.all([
        chmod(path.join(binDir, "mktemp"), 0o755),
        chmod(path.join(binDir, "nohup"), 0o755),
        chmod(path.join(binDir, "swift"), 0o755),
      ]);

      const result = await runCommand("sh", ["dist/mybin/tada"], {
        ...process.env,
        PATH: `${binDir}:${process.env.PATH ?? ""}`,
        TADA_MKTEMP_BIN: path.join(binDir, "mktemp"),
        TADA_NOHUP_BIN: path.join(binDir, "nohup"),
        TADA_SWIFT_BIN: path.join(binDir, "swift"),
        TADA_UNAME: "Darwin",
      });

      expect(result.code).toBe(0);
      for (let attempts = 0; attempts < 50; attempts += 1) {
        try {
          expect(await readFile(scriptCapturePath, "utf8")).toContain("import AppKit");
          return;
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
            throw error;
          }
          await Bun.sleep(20);
        }
      }
      expect(await readFile(scriptCapturePath, "utf8")).toContain("import AppKit");
    });
  });
});
