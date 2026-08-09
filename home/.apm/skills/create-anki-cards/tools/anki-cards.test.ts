import { afterEach, describe, expect, test } from "bun:test";
import path from "node:path";

type StoredNote = {
  noteId: number;
  modelName: string;
  fields: Record<string, string>;
  tags: string[];
  cards: number[];
};

type MockOptions = {
  apiKey?: string;
  fieldNames?: string[];
  multiFailureNoteId?: number;
  multiResultLimit?: number;
  verificationMismatch?: boolean;
};

const toolPath = path.join(import.meta.dir, "anki-cards.ts");
const servers: Array<ReturnType<typeof Bun.serve>> = [];

afterEach(() => {
  for (const server of servers.splice(0)) {
    server.stop(true);
  }
});

function validBatch() {
  return {
    version: 1,
    contract: {
      noteType: "基本",
      fields: [
        { name: "表面", role: "question", required: true },
        { name: "補足", role: "reference", required: false },
        { name: "裏面", role: "answer", required: true },
      ],
      tagPolicy: {
        mode: "restricted",
        allowed: ["株式", "経済"],
        requireAtLeastOne: true,
      },
    },
    notes: [
      {
        id: "market-001",
        deck: "投資",
        fields: {
          表面: "1日の約定値を取引成立順に並べた記録を何と呼ぶ？",
          補足: "https://example.com/primary",
          裏面: "歩み値",
        },
        tags: ["株式"],
        sources: ["https://example.com/primary"],
      },
    ],
  };
}

function createMock(options: MockOptions = {}) {
  const fieldNames = options.fieldNames ?? ["表面", "補足", "裏面"];
  const notes = new Map<number, StoredNote>();
  const cardDecks = new Map<number, string>();
  const requests: Array<{ action: string; params: any; key?: string }> = [];
  let nextNoteId = 200;
  let nextCardId = 1200;
  let mutated = false;

  function addStoredNote(note: StoredNote, deck = "投資") {
    notes.set(note.noteId, structuredClone(note));
    for (const cardId of note.cards) {
      cardDecks.set(cardId, deck);
    }
  }

  addStoredNote({
    noteId: 100,
    modelName: "基本",
    fields: { 表面: "古い問題", 補足: "古い補足", 裏面: "古い答え" },
    tags: ["経済"],
    cards: [1100],
  });
  addStoredNote({
    noteId: 101,
    modelName: "基本",
    fields: { 表面: "別の問題", 補足: "別の補足", 裏面: "別の答え" },
    tags: ["経済"],
    cards: [1101],
  });

  function noteInfo(note: StoredNote) {
    const fields = Object.fromEntries(
      fieldNames.map((name, order) => [
        name,
        {
          value:
            options.verificationMismatch && mutated && name === "裏面"
              ? "不一致"
              : note.fields[name] ?? "",
          order,
        },
      ]),
    );
    return {
      noteId: note.noteId,
      profile: "User 1",
      tags: note.tags,
      fields,
      modelName: note.modelName,
      mod: 1,
      cards: note.cards,
    };
  }

  async function handle(action: string, params: any): Promise<any> {
    if (action === "version") return 5;
    if (action === "getActiveProfile") return "User 1";
    if (action === "deckNames") return ["既定", "投資"];
    if (action === "modelNames") return ["基本"];
    if (action === "modelFieldNames") return fieldNames;
    if (action === "getTags") return ["経済", "株式"];
    if (action === "findNotes") return [...notes.keys()];
    if (action === "notesInfo") {
      return params.notes.map((noteId: number) => {
        const note = notes.get(noteId);
        return note === undefined ? {} : noteInfo(note);
      });
    }
    if (action === "cardsInfo") {
      return params.cards.map((cardId: number) => {
        const note = [...notes.values()].find((candidate) =>
          candidate.cards.includes(cardId),
        );
        return note === undefined
          ? {}
          : { cardId, note: note.noteId, deckName: cardDecks.get(cardId) };
      });
    }
    if (action === "multi") {
      const results = [];
      for (const request of params.actions) {
        if (options.apiKey !== undefined && request.key !== options.apiKey) {
          results.push({ result: null, error: "valid api key must be provided" });
          continue;
        }
        const noteId = request.params.note.id;
        if (noteId === options.multiFailureNoteId) {
          results.push({ result: null, error: "update failed" });
          continue;
        }
        const note = notes.get(noteId);
        if (note === undefined) {
          results.push({ result: null, error: "note not found" });
          continue;
        }
        note.fields = structuredClone(request.params.note.fields);
        note.tags = [...request.params.note.tags];
        mutated = true;
        results.push({ result: null, error: null });
      }
      return options.multiResultLimit === undefined
        ? results
        : results.slice(0, options.multiResultLimit);
    }
    if (action === "addNotes") {
      const result: number[] = [];
      for (const draft of params.notes) {
        const noteId = nextNoteId++;
        const cardId = nextCardId++;
        addStoredNote(
          {
            noteId,
            modelName: draft.modelName,
            fields: structuredClone(draft.fields),
            tags: [...draft.tags],
            cards: [cardId],
          },
          draft.deckName,
        );
        result.push(noteId);
      }
      mutated = true;
      return result;
    }
    throw new Error(`unsupported action: ${action}`);
  }

  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(request) {
      const payload = (await request.json()) as {
        action: string;
        params: any;
        key?: string;
      };
      requests.push(payload);
      if (options.apiKey !== undefined && payload.key !== options.apiKey) {
        return Response.json({ result: null, error: "valid api key must be provided" });
      }
      try {
        return Response.json({
          result: await handle(payload.action, payload.params),
          error: null,
        });
      } catch (error) {
        return Response.json({
          result: null,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    },
  });
  servers.push(server);
  return {
    notes,
    requests,
    url: `http://127.0.0.1:${server.port}`,
  };
}

async function runTool(
  command: string,
  options: {
    args?: string[];
    env?: Record<string, string>;
    input?: unknown;
    url?: string;
  } = {},
) {
  const env: Record<string, string | undefined> = {
    ...process.env,
    ANKI_CONNECT_URL: options.url,
    ...options.env,
  };
  delete env.ANKI_CONNECT_API_KEY;
  if (options.env?.ANKI_CONNECT_API_KEY !== undefined) {
    env.ANKI_CONNECT_API_KEY = options.env.ANKI_CONNECT_API_KEY;
  }
  const processHandle = Bun.spawn(
    [Bun.which("bun") as string, toolPath, command, ...(options.args ?? [])],
    {
      env,
      stderr: "pipe",
      stdin: "pipe",
      stdout: "pipe",
    },
  );
  if (options.input === undefined) {
    processHandle.stdin.end();
  } else {
    processHandle.stdin.write(JSON.stringify(options.input));
    processHandle.stdin.end();
  }
  const [exitCode, stdout, stderr] = await Promise.all([
    processHandle.exited,
    new Response(processHandle.stdout).text(),
    new Response(processHandle.stderr).text(),
  ]);
  return { exitCode, stderr, stdout };
}

describe("check", () => {
  test("標準入力のカードを検査する", async () => {
    const result = await runTool("check", { input: validBatch() });

    expect(result.exitCode).toBe(0);
    expect(result.stderr).toBe("");
    expect(JSON.parse(result.stdout)).toEqual({ checked: 1, warnings: [] });
  });

  test("フィールド、タグ、一次資料をまとめて検査する", async () => {
    const batch: any = validBatch();
    delete batch.notes[0].fields.裏面;
    batch.notes[0].fields.未知 = "値";
    batch.notes[0].tags = ["未許可"];
    batch.notes[0].sources = [];

    const result = await runTool("check", { input: batch });

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("fields.裏面: フィールドがありません");
    expect(result.stderr).toContain("fields.未知: 契約にないフィールドです");
    expect(result.stderr).toContain("許可されていないタグです");
    expect(result.stderr).toContain("一次資料がありません");
  });

  test("品質警告をJSONで返す", async () => {
    const batch: any = validBatch();
    batch.notes[0].fields.表面 = "何と何をそれぞれ答える？";

    const result = await runTool("check", { input: batch });

    expect(result.exitCode).toBe(0);
    expect(JSON.parse(result.stdout).warnings).toEqual([
      {
        noteId: "market-001",
        code: "multiple-recall",
        message: "複数回答または二重質問の可能性があります",
      },
    ]);
  });

  test("questionとanswerのroleを必須にする", async () => {
    const batch: any = validBatch();
    batch.contract.fields[0].role = "other";
    batch.contract.fields[2].role = "other";

    const result = await runTool("check", { input: batch });

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("question roleが必要です");
    expect(result.stderr).toContain("answer roleが必要です");
  });
});

describe("context", () => {
  test("既存構成と検索したノートをAnkiから取得する", async () => {
    const mock = createMock();

    const result = await runTool("context", {
      args: ["--query", 'deck:"投資"'],
      url: mock.url,
    });

    expect(result.exitCode).toBe(0);
    const output = JSON.parse(result.stdout);
    expect(output.apiVersion).toBe(5);
    expect(output.profile).toBe("User 1");
    expect(output.decks).toEqual(["投資", "既定"]);
    expect(output.noteTypes).toEqual([
      { name: "基本", fields: ["表面", "補足", "裏面"] },
    ]);
    expect(output.notes.map((note: any) => note.noteId)).toEqual([100, 101]);
    expect(mock.requests.some((request) => request.action === "findNotes")).toBe(true);
  });

  test("API keyをAnkiConnectへ渡す", async () => {
    const mock = createMock({ apiKey: "secret" });

    const result = await runTool("context", {
      env: { ANKI_CONNECT_API_KEY: "secret" },
      url: mock.url,
    });

    expect(result.exitCode).toBe(0);
    expect(mock.requests.every((request) => request.key === "secret")).toBe(true);
  });
});

describe("apply", () => {
  test("更新してから追加し、再取得した内容を検証する", async () => {
    const mock = createMock();
    const batch: any = validBatch();
    batch.notes.unshift({
      id: "market-000",
      noteId: 100,
      fields: {
        表面: "更新した問題は？",
        補足: "https://example.com/update",
        裏面: "更新した答え",
      },
      tags: ["株式"],
      sources: ["https://example.com/update"],
    });

    const result = await runTool("apply", {
      input: batch,
      url: mock.url,
    });

    expect(result.exitCode).toBe(0);
    expect(JSON.parse(result.stdout)).toEqual({
      created: [200],
      updated: [100],
      warnings: [],
    });
    expect(mock.notes.get(100)?.fields.裏面).toBe("更新した答え");
    expect(mock.notes.get(200)?.fields.裏面).toBe("歩み値");
    const actions = mock.requests.map((request) => request.action);
    expect(actions.indexOf("multi")).toBeLessThan(actions.indexOf("addNotes"));
    expect(actions.at(-1)).toBe("cardsInfo");
  });

  test("未確認の品質警告があれば接続前に拒否する", async () => {
    const mock = createMock();
    const batch: any = validBatch();
    batch.notes[0].fields.表面 = "何と何をそれぞれ答える？";

    const result = await runTool("apply", { input: batch, url: mock.url });

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("reasonに例外理由を記録してください");
    expect(mock.requests).toEqual([]);
  });

  test("API keyを複数更新の内側にも渡す", async () => {
    const mock = createMock({ apiKey: "secret" });
    const batch: any = validBatch();
    batch.notes = [
      {
        id: "market-100",
        noteId: 100,
        fields: batch.notes[0].fields,
        tags: batch.notes[0].tags,
        sources: batch.notes[0].sources,
      },
    ];

    const result = await runTool("apply", {
      env: { ANKI_CONNECT_API_KEY: "secret" },
      input: batch,
      url: mock.url,
    });

    expect(result.exitCode).toBe(0);
    const multi = mock.requests.find((request) => request.action === "multi");
    expect(multi?.params.actions[0].key).toBe("secret");
  });

  test("更新の一部が失敗したら追加せず現在状態の再取得を求める", async () => {
    const mock = createMock({ multiFailureNoteId: 101 });
    const batch: any = validBatch();
    batch.notes = [
      {
        ...batch.notes[0],
        id: "market-100",
        noteId: 100,
        deck: undefined,
      },
      {
        ...structuredClone(batch.notes[0]),
        id: "market-101",
        noteId: 101,
        deck: undefined,
      },
      {
        ...structuredClone(batch.notes[0]),
        id: "market-new",
      },
    ];

    const result = await runTool("apply", { input: batch, url: mock.url });

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("更新が一部失敗しました");
    expect(result.stderr).toContain("現在状態を再取得してください");
    expect(mock.requests.some((request) => request.action === "addNotes")).toBe(false);
    expect(mock.notes.get(100)?.fields.裏面).toBe("歩み値");
    expect(mock.notes.get(101)?.fields.裏面).toBe("別の答え");
  });

  test("全更新の応答がなければ追加しない", async () => {
    const mock = createMock({ multiResultLimit: 0 });
    const batch: any = validBatch();
    batch.notes.unshift({
      id: "market-100",
      noteId: 100,
      fields: structuredClone(batch.notes[0].fields),
      tags: [...batch.notes[0].tags],
      sources: [...batch.notes[0].sources],
    });

    const result = await runTool("apply", { input: batch, url: mock.url });

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("全更新の結果を返しませんでした");
    expect(mock.requests.some((request) => request.action === "addNotes")).toBe(false);
  });

  test("Ankiのフィールド契約が違えば書き込まない", async () => {
    const mock = createMock({ fieldNames: ["Front", "Back"] });

    const result = await runTool("apply", {
      input: validBatch(),
      url: mock.url,
    });

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("フィールド契約が一致しません");
    expect(mock.requests.some((request) => request.action === "addNotes")).toBe(false);
  });

  test("再取得した内容が違えば失敗する", async () => {
    const mock = createMock({ verificationMismatch: true });

    const result = await runTool("apply", {
      input: validBatch(),
      url: mock.url,
    });

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("書き込み後のフィールドが一致しません");
  });

  test("loopback以外への接続を拒否する", async () => {
    const result = await runTool("apply", {
      input: validBatch(),
      url: "https://example.com",
    });

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("loopbackのHTTP originだけ");
  });
});
