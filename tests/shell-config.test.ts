import { describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";

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

    expect(globalMiseConfig).toContain('direnv = "2.37.1"');
    expect(globalMiseConfig).toContain('ghq = "1.9.4"');
    expect(globalMiseConfig).toContain('fzf = "0.70.0"');
    expect(globalMiseConfig).toContain('ripgrep = "15.1.0"');
  });

  test("zshenv does not require go and guards kubectl", async () => {
    const zshenv = await readFile("dist/.zshenv", "utf8");

    expect(zshenv).not.toContain("go env GOPATH");
    expect(zshenv).not.toContain("autoload -U +X compinit && compinit");
    expect(zshenv).toContain("if command -v kubectl > /dev/null 2>&1; then");
    expect(zshenv).toContain('source <(kubectl completion zsh)');
  });

  test("zshenv sets up Homebrew on macOS as well as Linuxbrew", async () => {
    const zshenv = await readFile("dist/.zshenv", "utf8");

    expect(zshenv).toContain("[ -f '/opt/homebrew/bin/brew' ]");
    expect(zshenv).toContain("eval \"$(/opt/homebrew/bin/brew shellenv)\"");
    expect(zshenv).toContain("[ -f '/home/linuxbrew/.linuxbrew/bin/brew' ]");
  });

  test("zshrc guards direnv hook behind a resolvable mise tool", async () => {
    const zshrc = await readFile("dist/.zshrc", "utf8");

    expect(zshrc).toContain("if mise which direnv > /dev/null 2>&1; then");
    expect(zshrc).toContain('eval "$(direnv hook zsh)"');
  });

  test("zshrc only loads zinit when the installed file exists", async () => {
    const zshrc = await readFile("dist/.zshrc", "utf8");

    expect(zshrc).toContain('if [ -f "${ZINIT_HOME}/zinit.zsh" ]; then');
    expect(zshrc).not.toContain("git clone https://github.com/zdharma-continuum/zinit.git");
    expect(zshrc).toContain("autoload -U +X compinit && compinit");
  });

  test("git credential helper resolves gh via PATH", async () => {
    const gitconfig = await readFile("dist/.gitconfig", "utf8");

    expect(gitconfig).toContain("helper = !gh auth git-credential");
    expect(gitconfig).not.toContain("/usr/local/bin/gh auth git-credential");
  });
});
