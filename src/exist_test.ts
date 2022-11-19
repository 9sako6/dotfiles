import { assertEquals } from "../test_deps.ts";
import { exist } from "./exist.ts";

Deno.test("exist", async (t) => {
  await t.step("正常系", async (t) => {
    await t.step("ファイルの存在を検知できる", async () => {
      const tmpFilePath = await Deno.makeTempFile();
      assertEquals(await exist(tmpFilePath), true);
    });

    await t.step("ファイルの不在を検知できる", async () => {
      const tmpFilePath = await Deno.makeTempFile();
      assertEquals(await exist(tmpFilePath + ".not"), false);
    });

    await t.step("ディレクトリの存在を検知できる", async () => {
      const tmpDirPath = await Deno.makeTempDir();
      assertEquals(await exist(tmpDirPath), true);
    });

    await t.step("ディレクトリの不在を検知できる", async () => {
      const tmpDirPath = await Deno.makeTempDir();
      assertEquals(await exist(tmpDirPath + ".not"), false);
    });
  });
});
