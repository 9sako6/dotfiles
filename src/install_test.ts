import { assert } from "../test_deps.ts";
import { install } from "./install.ts";

Deno.test("install", async (t) => {
  await t.step("正常系", async (t) => {
    await t.step("設定ファイルをシンボリックリンクで配置できる", async (t) => {
      const sourceDir = await Deno.makeTempDir();
      await Deno.writeTextFile(`${sourceDir}/.zshrc`, "dummy");
      await Deno.mkdir(`${sourceDir}/zsh.dir`);

      const destDir = await Deno.makeTempDir();

      await install(sourceDir, destDir);

      assert((await Deno.stat(`${destDir}/.zshrc`)).isFile);
      assert((await Deno.stat(`${destDir}/zsh.dir`)).isDirectory);
    });
  });
});
