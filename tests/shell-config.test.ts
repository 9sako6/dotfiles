import { describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";

describe("shell config", () => {
  test("mise tools include direnv and not go", async () => {
    const miseToml = await readFile(".mise.toml", "utf8");

    expect(miseToml).toContain('direnv = "latest"');
    expect(miseToml).not.toContain('\ngo = "latest"\n');
  });

  test("zshenv does not require go and guards kubectl", async () => {
    const zshenv = await readFile("dist/.zshenv", "utf8");

    expect(zshenv).not.toContain("go env GOPATH");
    expect(zshenv).toContain("if command -v kubectl > /dev/null 2>&1; then");
    expect(zshenv).toContain('source <(kubectl completion zsh)');
  });
});
