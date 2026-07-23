import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

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
      preview: "cards.preview.md",
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

describe("build", () => {
  test("任意のフィールド順を保ち、安全な新規作成TSVとプレビューを生成する", async () => {
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

    const preview = await readFile(
      path.join(directory, "cards.preview.md"),
      "utf8",
    );
    expect(preview).toContain("## market-001");
    expect(preview).toContain("### 表面");
    expect(preview).toContain("https://example.com/primary");
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

  test("検証に失敗したとき既存の生成物を変更しない", async () => {
    const project = validProject();
    project.cards.push(structuredClone(project.cards[0]));
    const { directory, inputPath } = await createProject(project);
    const tsvPath = path.join(directory, "cards.tsv");
    const previewPath = path.join(directory, "cards.preview.md");
    await writeFile(tsvPath, "以前のTSV\n");
    await writeFile(previewPath, "以前のプレビュー\n");

    const result = await runTool(directory, "build", inputPath);

    expect(result.exitCode).toBe(1);
    expect(await readFile(tsvPath, "utf8")).toBe("以前のTSV\n");
    expect(await readFile(previewPath, "utf8")).toBe("以前のプレビュー\n");
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
});

describe("check", () => {
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

  test("契約の版、出力先、フィールド、更新識別子を検証する", async () => {
    const project: any = validProject();
    project.version = 2;
    project.contract.mode = "update";
    project.contract.output = "../cards.tsv";
    project.contract.fields.push({
      name: "表面",
      role: "other",
      required: false,
    });
    const { directory, inputPath } = await createProject(project);

    const result = await runTool(directory, "check", inputPath);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("version: 対応している値は1だけです");
    expect(result.stderr).toContain(
      "contract.output: 作業ディレクトリ内の相対パスを指定してください",
    );
    expect(result.stderr).toContain(
      "contract.fields[3].name: フィールド名が重複しています: 表面",
    );
    expect(result.stderr).toContain(
      "contract.identityField: 更新モードでは必須です",
    );
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
