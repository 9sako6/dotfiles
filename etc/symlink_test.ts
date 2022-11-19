import { NameMismatchError, setSymlink } from "./symlink.ts";
import { basename } from "../deps.ts";
import { assert, assertRejects } from "../test_deps.ts";

Deno.test("setSymlink", async (t) => {
  await t.step("正常系", async (t) => {
    await t.step("ファイルのシンボリックリンクが配置される", async () => {
      const srcFile = await Deno.makeTempFile({
        suffix: ".txt",
      });
      const srcFileName = basename(srcFile);
      const destDirPath = await Deno.makeTempDir();
      const destFile = `${destDirPath}/${srcFileName}`;

      await setSymlink(srcFile, destFile);

      const stat = await Deno.stat(destFile);
      assert(stat.isFile);
    });

    await t.step("ディレクトリのシンボリックリンクが配置される", async () => {
      const srcDirPath = await Deno.makeTempDir();
      const srcDirName = basename(srcDirPath);
      const destDirPath = await Deno.makeTempDir();
      const destPath = `${destDirPath}/${srcDirName}`;

      await setSymlink(srcDirPath, destPath);

      const stat = await Deno.stat(destPath);
      assert(stat.isDirectory);
    });

    await t.step("すでに中身のあるディレクトリが存在する場合は削除して新たなシンボリックリンクが配置される", async () => {
      const srcDir = await Deno.makeTempDir();
      const destDir = await Deno.makeTempDir();
      await Deno.mkdir(`${srcDir}/dir`);
      await Deno.mkdir(`${destDir}/dir`);
      await Deno.writeTextFile(`${destDir}/dir/dummy.txt`, "dummy");

      await setSymlink(`${srcDir}/dir`, `${destDir}/dir`);

      const stat = await Deno.stat(`${destDir}/dir`);
      assert(stat.isDirectory);
    });
  });

  await t.step("異常系", async (t) => {
    await t.step(
      "ファイルのシンボリックリンク配置先として、同じファイル名で終わるパスを指定しないとエラーになる",
      async () => {
        const srcFile = await Deno.makeTempFile({
          suffix: ".txt",
        });
        const destDir = await Deno.makeTempDir();

        assertRejects(
          async () => await setSymlink(srcFile, destDir),
          NameMismatchError,
        );
      },
    );

    await t.step(
      "ディレクトリのシンボリックリンク配置先として、同じディレクトリ名で終わるパスを指定しないとエラーになる",
      async () => {
        const srcDir = await Deno.makeTempFile();
        const destDir = await Deno.makeTempDir();

        assertRejects(
          async () => await setSymlink(srcDir, destDir),
          NameMismatchError,
        );
      },
    );
  });
});
