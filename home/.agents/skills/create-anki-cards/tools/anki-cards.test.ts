import { afterEach, describe, expect, test } from "bun:test";
import {
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { ankiBase91 } from "./anki-cards";

const toolPath = path.join(import.meta.dir, "anki-cards.ts");
const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((directory) =>
      rm(directory, { force: true, recursive: true }),
    ),
  );
});

async function createProject(data: unknown): Promise<{
  directory: string;
  inputPath: string;
}> {
  const directory = await mkdtemp(path.join(os.tmpdir(), "anki-cards-test-"));
  temporaryDirectories.push(directory);
  const inputPath = path.join(directory, "anki.json");
  await writeFile(inputPath, `${JSON.stringify(data, null, 2)}\n`);
  return { directory, inputPath };
}

async function runTool(
  directory: string,
  ...args: string[]
): Promise<{ exitCode: number; stderr: string; stdout: string }> {
  const process = Bun.spawn([Bun.argv[0], toolPath, ...args], {
    cwd: directory,
    stderr: "pipe",
    stdout: "pipe",
  });
  const [exitCode, stderr, stdout] = await Promise.all([
    process.exited,
    new Response(process.stderr).text(),
    new Response(process.stdout).text(),
  ]);
  return { exitCode, stderr, stdout };
}

function validProject() {
  return {
    version: 1,
    contract: {
      mode: "create",
      output: "cards.tsv",
      deck: "投資",
      noteType: "基本",
      html: true,
      fields: [
        { name: "表面", role: "question", required: true },
        { name: "補足", role: "other", required: false },
        { name: "裏面", role: "answer", required: true },
      ],
      tagPolicy: {
        mode: "restricted",
        allowed: ["経済", "株式"],
        requireAtLeastOne: true,
      },
    },
    cards: [
      {
        id: "market-001",
        fields: {
          表面: "定義\tを何と呼ぶ？",
          補足: "一行目\n「引用」",
          裏面: "答え",
        },
        tags: ["株式", "経済"],
        sources: ["https://example.com/primary"],
      },
    ],
  };
}

describe("カードの生成", () => {
  test("任意のフィールド順を保ち、安全な新規作成TSVを生成する", async () => {
    const { directory, inputPath } = await createProject(validProject());

    const result = await runTool(directory, "build", inputPath);

    expect(result).toEqual({ exitCode: 0, stderr: "", stdout: "Built 1 card.\n" });
    const tsv = await readFile(path.join(directory, "cards.tsv"), "utf8");
    expect(tsv).toBe(
      [
        "#separator:tab",
        "#html:true",
        "#notetype:基本",
        "#deck:投資",
        "#tags column:4",
        "#columns:表面\t補足\t裏面\tTags",
        '"定義\tを何と呼ぶ？"\t"一行目\n「引用」"\t答え\t株式 経済',
        "",
      ].join("\n"),
    );
    expect(tsv).not.toContain("#guid column:");
  });

  test("Anki互換GUIDを生成して正規データへ保存し、再生成でも維持する", async () => {
    const project: any = validProject();
    project.contract.guidPolicy = "generate";
    const { directory, inputPath } = await createProject(project);

    const first = await runTool(directory, "build", inputPath);

    expect(first.exitCode).toBe(0);
    const generated = JSON.parse(await readFile(inputPath, "utf8"));
    const guid = generated.cards[0].guid;
    const table =
      "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!#$%&()*+,-./:;<=>?@[]^_`{|}~";
    expect(guid.length).toBeGreaterThan(0);
    expect(guid.length).toBeLessThanOrEqual(10);
    expect([...guid].every((character) => table.includes(character))).toBe(
      true,
    );
    const firstTsv = await readFile(path.join(directory, "cards.tsv"), "utf8");
    expect(firstTsv).toContain("#guid column:1\n");
    expect(firstTsv).toContain(`${guid}\t`);

    const second = await runTool(directory, "build", inputPath);

    expect(second.exitCode).toBe(0);
    const rebuilt = JSON.parse(await readFile(inputPath, "utf8"));
    expect(rebuilt.cards[0].guid).toBe(guid);
  });

  test("Anki書き出しと完全照合し、Anki由来GUIDで更新TSVを作る", async () => {
    const project: any = validProject();
    project.contract.mode = "update";
    project.contract.identityField = "識別子";
    project.contract.fields[1] = {
      name: "識別子",
      role: "id",
      required: true,
    };
    delete project.cards[0].fields.補足;
    project.cards[0].fields.表面 = "定義を何と呼ぶ？";
    project.cards[0].fields.識別子 = "source-1";
    const { directory, inputPath } = await createProject(project);
    const exportPath = path.join(directory, "exported.tsv");
    await writeFile(
      exportPath,
      [
        "#separator:tab",
        "#html:true",
        "#guid column:1",
        "#notetype:基本",
        "#columns:GUID\t表面\t識別子\t裏面\tTags",
        "anki-guid-1\t古い表面\tsource-1\t古い答え\t株式",
        "",
      ].join("\n"),
    );

    const result = await runTool(
      directory,
      "build",
      inputPath,
      "--anki-export",
      exportPath,
    );

    expect(result.exitCode).toBe(0);
    const tsv = await readFile(path.join(directory, "cards.tsv"), "utf8");
    expect(tsv).toContain("#guid column:1\n");
    expect(tsv).toContain(
      "anki-guid-1\t定義を何と呼ぶ？\tsource-1\t答え\t株式 経済\n",
    );
  });

  test("Anki書き出しに不足または余剰があれば更新TSVを作らない", async () => {
    const project: any = validProject();
    project.contract.mode = "update";
    project.contract.identityField = "補足";
    const { directory, inputPath } = await createProject(project);
    const exportPath = path.join(directory, "exported.tsv");
    await writeFile(
      exportPath,
      [
        "#separator:tab",
        "#guid column:1",
        "#columns:GUID\t表面\t補足\t裏面\tTags",
        'anki-guid-1\t古い表面\t"一行目\n「引用」"\t古い答え\t株式',
        "anki-guid-2\t余分\t余分な識別値\t余分\t株式",
        "",
      ].join("\n"),
    );

    const result = await runTool(
      directory,
      "build",
      inputPath,
      "--anki-export",
      exportPath,
    );

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("余剰: 余分な識別値");
    expect(Bun.file(path.join(directory, "cards.tsv")).size).toBe(0);
  });

  test("異なるノートタイプのAnki書き出しを更新に使えない", async () => {
    const project: any = validProject();
    project.contract.mode = "update";
    project.contract.identityField = "補足";
    const { directory, inputPath } = await createProject(project);
    const exportPath = path.join(directory, "exported.tsv");
    await writeFile(
      exportPath,
      [
        "#separator:tab",
        "#guid column:1",
        "#notetype:穴埋め問題",
        "#columns:GUID\t表面\t補足\t裏面\tTags",
        'anki-guid-1\t古い表面\t"一行目\n「引用」"\t古い答え\t株式',
        "",
      ].join("\n"),
    );

    const result = await runTool(
      directory,
      "build",
      inputPath,
      "--anki-export",
      exportPath,
    );

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain(
      "ノートタイプが契約と一致しません: 穴埋め問題",
    );
  });

  test("ノートタイプ列も契約と照合する", async () => {
    const project: any = validProject();
    project.contract.mode = "update";
    project.contract.identityField = "補足";
    const { directory, inputPath } = await createProject(project);
    const exportPath = path.join(directory, "exported.tsv");
    await writeFile(
      exportPath,
      [
        "#separator:tab",
        "#guid column:1",
        "#notetype column:2",
        "#columns:GUID\tNotetype\t表面\t補足\t裏面\tTags",
        'anki-guid-1\t穴埋め問題\t古い表面\t"一行目\n「引用」"\t古い答え\t株式',
        "",
      ].join("\n"),
    );

    const result = await runTool(
      directory,
      "build",
      inputPath,
      "--anki-export",
      exportPath,
    );

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain(
      "1行目のノートタイプが契約と一致しません: 穴埋め問題",
    );
  });

  test("検証に失敗したとき既存の生成物をTSVを変更しない", async () => {
    const project = validProject();
    project.cards.push(structuredClone(project.cards[0]));
    const { directory, inputPath } = await createProject(project);
    const tsvPath = path.join(directory, "cards.tsv");
    await writeFile(tsvPath, "以前のTSV\n");

    const result = await runTool(directory, "build", inputPath);

    expect(result.exitCode).toBe(1);
    expect(await readFile(tsvPath, "utf8")).toBe("以前のTSV\n");
  });

  test("一つの出力先を置き換えられない場合は正規データと既存出力を変更しない", async () => {
    const project: any = validProject();
    project.contract.guidPolicy = "generate";
    const { directory, inputPath } = await createProject(project);
    const original = await readFile(inputPath, "utf8");
    const tsvPath = path.join(directory, "cards.tsv");
    await rm(tsvPath, { force: true });
    await mkdir(tsvPath);

    const result = await runTool(directory, "build", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("出力先をdirectoryで置き換えられません");
    expect(await readFile(inputPath, "utf8")).toBe(original);
  });

  test("正規データ自身を出力先に指定できない", async () => {
    const project = validProject();
    project.contract.output = "anki.json";
    const { directory, inputPath } = await createProject(project);
    const original = await readFile(inputPath, "utf8");

    const result = await runTool(directory, "build", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("正規データ自身を上書きできません");
    expect(await readFile(inputPath, "utf8")).toBe(original);
  });

  test("project外を指すsymlink parentへの出力を拒否する", async () => {
    const project = validProject();
    project.contract.output = "out/cards.tsv";
    const { directory, inputPath } = await createProject(project);
    const outside = await mkdtemp(path.join(os.tmpdir(), "anki-cards-outside-"));
    temporaryDirectories.push(outside);
    const outsideOutput = path.join(outside, "cards.tsv");
    await writeFile(outsideOutput, "変更前\n");
    await symlink(outside, path.join(directory, "out"));

    const result = await runTool(directory, "build", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("出力先の親directoryがproject外です");
    expect(await readFile(outsideOutput, "utf8")).toBe("変更前\n");
    expect(Bun.file(path.join(directory, "cards.tsv")).size).toBe(0);
  });

  test("project内の通常directoryではatomic replacementを維持する", async () => {
    const project = validProject();
    project.contract.output = "generated/cards.tsv";
    const { directory, inputPath } = await createProject(project);
    const generated = path.join(directory, "generated");
    await mkdir(generated);
    await writeFile(path.join(generated, "cards.tsv"), "変更前\n");

    const result = await runTool(directory, "build", inputPath);

    expect(result.exitCode).toBe(0);
    expect(await readFile(path.join(generated, "cards.tsv"), "utf8")).toContain(
      "#separator:tab",
    );
  });
});

describe("Anki GUIDの生成", () => {
  test("Anki本体と同じbase91表現を生成する", () => {
    expect(ankiBase91(0n)).toBe("");
    expect(ankiBase91(1n)).toBe("b");
    expect(ankiBase91(2n ** 64n - 1n)).toBe("Rj&Z5m[>Zp");
    expect(ankiBase91(1234567890n)).toBe("saAKk");
  });
});

describe("カードの検証", () => {
  test("レビューIDの重複を拒否する", async () => {
    const project = validProject();
    project.cards.push(structuredClone(project.cards[0]));
    const { directory, inputPath } = await createProject(project);

    const result = await runTool(directory, "check", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stdout).toBe("");
    expect(result.stderr).toContain(
      "cards[1].id: レビューIDが重複しています: market-001",
    );
  });

  test("不足フィールドと契約にないフィールドを同時に報告する", async () => {
    const project: any = validProject();
    delete project.cards[0].fields.裏面;
    project.cards[0].fields.未知 = "出力してはいけない";
    const { directory, inputPath } = await createProject(project);

    const result = await runTool(directory, "check", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain(
      "cards[0].fields.裏面: 必須フィールドがありません",
    );
    expect(result.stderr).toContain(
      "cards[0].fields.未知: 契約にないフィールドです",
    );
  });

  test("タグ規約違反と一次資料の欠落を拒否する", async () => {
    const project = validProject();
    project.cards[0].tags = ["株式", "株式", "未許可"];
    project.cards[0].sources = [];
    const { directory, inputPath } = await createProject(project);

    const result = await runTool(directory, "check", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("cards[0].tags: タグが重複しています: 株式");
    expect(result.stderr).toContain("cards[0].tags: 許可されていないタグです: 未許可");
    expect(result.stderr).toContain("cards[0].sources: 一次資料がありません");
  });

  test("文章品質の疑いは警告し、構造が正しければ成功する", async () => {
    const project = validProject();
    project.cards[0].fields.表面 =
      "AとBをそれぞれ何と呼ぶ？ それはいくつある？";
    project.cards[0].fields.裏面 = "長い答え".repeat(25);
    const { directory, inputPath } = await createProject(project);

    const result = await runTool(directory, "check", inputPath);

    expect(result.exitCode).toBe(0);
    expect(result.stdout).toBe("Checked 1 card: 2 warnings.\n");
    expect(result.stderr).toContain("Warning [market-001/multiple-recall]");
    expect(result.stderr).toContain("Warning [market-001/long-answer]");
  });

  test("契約の版と更新識別子をschemaで検証する", async () => {
    const project: any = validProject();
    project.version = 2;
    project.contract.mode = "update";
    const { directory, inputPath } = await createProject(project);

    const result = await runTool(directory, "check", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("version: 対応している値は1だけです");
    expect(result.stderr).toContain("contract.identityField: 空でない文字列が必要です");
  });

  test("出力先とフィールドのsemantic contractを検証する", async () => {
    const project: any = validProject();
    project.contract.mode = "update";
    project.contract.identityField = "補足";
    project.contract.output = "../cards.tsv";
    project.contract.fields.push({
      name: "表面",
      role: "other",
      required: false,
    });
    const { directory, inputPath } = await createProject(project);

    const result = await runTool(directory, "check", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain(
      "contract.output: 作業ディレクトリ内の相対パスを指定してください",
    );
    expect(result.stderr).toContain(
      "contract.fields[3].name: フィールド名が重複しています: 表面",
    );
  });

  test("schemaの全階層で未知のキーをpath付きで拒否する", async () => {
    const project: any = validProject();
    project.extraRoot = true;
    project.contract.guidPolciy = "generate";
    project.contract.fields[0].extraField = true;
    project.contract.tagPolicy.extraPolicy = true;
    project.cards[0].extraCard = true;
    const { directory, inputPath } = await createProject(project);

    const result = await runTool(directory, "check", inputPath);

    expect(result.exitCode).toBe(1);
    for (const pathName of [
      "root.extraRoot",
      "contract.guidPolciy",
      "contract.fields[0].extraField",
      "contract.tagPolicy.extraPolicy",
      "cards[0].extraCard",
    ]) {
      expect(result.stderr).toContain(`${pathName}: 未知のキーです`);
    }
  });

  test("createとupdateの無効な契約の組合せを拒否する", async () => {
    const createProjectData: any = validProject();
    createProjectData.contract.identityField = "補足";
    const createFixture = await createProject(createProjectData);

    const createResult = await runTool(
      createFixture.directory,
      "check",
      createFixture.inputPath,
    );

    expect(createResult.exitCode).toBe(1);
    expect(createResult.stderr).toContain("contract.identityField: 未知のキーです");

    const updateProjectData: any = validProject();
    updateProjectData.contract.mode = "update";
    updateProjectData.contract.identityField = "補足";
    updateProjectData.contract.guidPolicy = "generate";
    updateProjectData.cards[0].guid = "abc";
    const updateFixture = await createProject(updateProjectData);

    const updateResult = await runTool(
      updateFixture.directory,
      "check",
      updateFixture.inputPath,
    );

    expect(updateResult.exitCode).toBe(1);
    expect(updateResult.stderr).toContain("contract.guidPolicy: 未知のキーです");
    expect(updateResult.stderr).toContain(
      "cards[0].guid: createモードかつguidPolicyがgenerateの場合だけ指定できます",
    );
  });

  test("sourceは公開URLまたはrepository相対参照だけを受け付ける", async () => {
    const accepted = validProject();
    accepted.cards[0].sources = [
      "docs/spec.md",
      "lib/parser.ts:42",
      "https://example.com/spec#section",
    ];
    const acceptedFixture = await createProject(accepted);

    const acceptedResult = await runTool(
      acceptedFixture.directory,
      "check",
      acceptedFixture.inputPath,
    );

    expect(acceptedResult.exitCode).toBe(0);

    const rejected = validProject();
    rejected.cards[0].sources = [
      "/Users/example/private.md",
      "../outside.md",
      "file:///tmp/spec.md",
      "vscode://file/spec.md",
      "~/spec.md",
      "$HOME/spec.md",
    ];
    const rejectedFixture = await createProject(rejected);

    const rejectedResult = await runTool(
      rejectedFixture.directory,
      "check",
      rejectedFixture.inputPath,
    );

    expect(rejectedResult.exitCode).toBe(1);
    for (let index = 0; index < rejected.cards[0].sources.length; index += 1) {
      expect(rejectedResult.stderr).toContain(
        `cards[0].sources[${index}]: 公開URLまたはrepository相対参照が必要です`,
      );
    }
  });

  test("壊れたJSON構造を内部エラーにせず報告する", async () => {
    const { directory, inputPath } = await createProject({
      version: 1,
      contract: null,
      cards: "not-an-array",
    });

    const result = await runTool(directory, "check", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("contract: オブジェクトが必要です");
    expect(result.stderr).toContain("cards: 配列が必要です");
    expect(result.stderr).not.toContain("TypeError");
  });

  test("Ankiへ安全に渡せない制御文字を拒否する", async () => {
    const project = validProject();
    project.cards[0].fields.表面 = "不正\u0000な問題";
    const { directory, inputPath } = await createProject(project);

    const result = await runTool(directory, "check", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain(
      "cards[0].fields.表面: 使用できない制御文字を含んでいます",
    );
  });

  test("更新モードの識別値重複を拒否する", async () => {
    const project: any = validProject();
    project.contract.mode = "update";
    project.contract.identityField = "補足";
    const second = structuredClone(project.cards[0]);
    second.id = "market-002";
    project.cards.push(second);
    const { directory, inputPath } = await createProject(project);

    const result = await runTool(directory, "check", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain(
      "cards[1].fields.補足: 更新識別値が重複しています",
    );
  });

  test("ヘッダー値、レビューID、タグに区切りを壊す空白を許さない", async () => {
    const project: any = validProject();
    project.contract.deck = "投資\n#guid column:1";
    project.contract.tagPolicy = {
      mode: "open",
      requireAtLeastOne: true,
    };
    project.cards[0].id = "bad id";
    project.cards[0].tags = ["bad tag"];
    const { directory, inputPath } = await createProject(project);

    const result = await runTool(directory, "check", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain(
      "contract.deck: 改行または制御文字を含められません",
    );
    expect(result.stderr).toContain(
      "cards[0].id: 空白または制御文字を含められません",
    );
    expect(result.stderr).toContain(
      "cards[0].tags[0]: 空白または制御文字を含められません",
    );
  });
});
