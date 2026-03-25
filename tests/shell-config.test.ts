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

  test("git config does not depend on GitHub CLI credential helpers", async () => {
    const gitconfig = await readFile("dist/.gitconfig", "utf8");

    expect(gitconfig).not.toContain("gh auth git-credential");
    expect(gitconfig).not.toContain('[credential "https://github.com"]');
    expect(gitconfig).not.toContain('[credential "https://gist.github.com"]');
  });

  test("shell aliases keep only currently supported tools", async () => {
    const aliases = await readFile("dist/alias.sh", "utf8");
    const abbrevAliases = await readFile("dist/.zsh.local/alias.zsh", "utf8");
    const prompt = await readFile("dist/.zsh.local/prompt.zsh", "utf8");

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
});
