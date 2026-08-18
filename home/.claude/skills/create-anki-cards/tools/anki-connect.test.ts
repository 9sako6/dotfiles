import { describe, expect, test } from "bun:test";

import {
  AnkiConnectClient,
  add,
  assertLocalEndpoint,
  parseAddInput,
  recover,
  snapshot,
} from "./anki-connect";

function response(result: unknown, error: string | null = null): Response {
  return new Response(JSON.stringify({ result, error }), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

function fakeClient(
  handler: (action: string, params: Record<string, unknown> | undefined) => Promise<Response> | Response,
): AnkiConnectClient {
  return new AnkiConnectClient(
    "http://127.0.0.1:8765",
    (async (_input, init) => {
      const request = JSON.parse(String(init?.body)) as {
        action: string;
        params?: Record<string, unknown>;
      };
      return handler(request.action, request.params);
    }) as typeof fetch,
  );
}

describe("endpoint", () => {
  test("accepts endpoints on the machine running Anki", () => {
    expect(() => assertLocalEndpoint("http://127.0.0.1:8765")).not.toThrow();
    expect(() => assertLocalEndpoint("http://[::1]:8765")).not.toThrow();
    expect(() => assertLocalEndpoint("http://host.docker.internal:8765")).not.toThrow();
    expect(() => assertLocalEndpoint("http://localhost:8765")).not.toThrow();
  });

  test("rejects endpoints on other hosts", () => {
    expect(() => assertLocalEndpoint("http://192.168.1.2:8765")).toThrow(
      "must be loopback or host.docker.internal",
    );
    expect(() => assertLocalEndpoint("http://anki.example.com:8765")).toThrow(
      "must be loopback or host.docker.internal",
    );
    expect(() => assertLocalEndpoint("https://127.0.0.1:8765")).toThrow("must use http");
  });
});

describe("snapshot", () => {
  test("reconstructs collection metadata and queried notes", async () => {
    const client = fakeClient((action) => {
      switch (action) {
        case "version": return response(6);
        case "deckNames": return response(["技術"]);
        case "getTags": return response(["quint", "invariant", "create-anki-cards-tx-stale"]);
        case "modelNames": return response(["Basic"]);
        case "findNotes": return response([10]);
        case "notesInfo": return response([{ noteId: 10, modelName: "Basic", tags: ["quint"], fields: { Front: { value: "Q", order: 0 }, Back: { value: "A", order: 1 } }, cards: [20] }]);
        case "cardsInfo": return response([{ cardId: 20, note: 10, deckName: "技術" }]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await snapshot(client, "tag:quint");
    expect(result.decks).toEqual(["技術"]);
    expect(result.pendingTransactions).toEqual(["create-anki-cards-tx-stale"]);
    expect((result.notes as unknown[]).length).toBe(1);
    expect((result.cards as unknown[]).length).toBe(1);
  });
});

describe("add", () => {
  test("allows an existing legacy tag but validates new tags", async () => {
    const existingClient = fakeClient((action) => {
      if (action === "getTags") return response(["Legacy_Tag"]);
      if (action === "canAddNotesWithErrorDetail") return response([{ canAdd: false, error: "stop" }]);
      throw new Error(`unexpected action ${action}`);
    });
    const result = await add(existingClient, {
      notes: [{ deckName: "技術", modelName: "Basic", fields: { Front: "Q", Back: "A" }, tags: ["Legacy_Tag"] }],
    });
    expect(result.status).toBe("blocked");

    const newTagClient = fakeClient((action) => {
      if (action === "getTags") return response([]);
      throw new Error(`unexpected action ${action}`);
    });
    await expect(add(newTagClient, {
      notes: [{ deckName: "技術", modelName: "Basic", fields: { Front: "Q", Back: "A" }, tags: ["Bad_Tag"] }],
    })).rejects.toThrow("lowercase letters");
  });

  test("adds with a transaction tag, verifies from Anki, then removes the tag", async () => {
    let transactionTag = "";
    const actions: string[] = [];
    const client = fakeClient((action, params) => {
      actions.push(action);
      switch (action) {
        case "getTags": return response(["quint"]);
        case "canAddNotesWithErrorDetail": {
          const notes = params?.notes as Array<{ tags: string[] }>;
          transactionTag = notes[0].tags.find((tag) => tag.startsWith("create-anki-cards-tx-")) ?? "";
          return response([{ canAdd: true, error: null }]);
        }
        case "addNotes": return response([101]);
        case "findNotes": return response([101]);
        case "notesInfo": return response([{ noteId: 101, modelName: "Basic", tags: ["quint", transactionTag], fields: { Front: { value: "Q", order: 0 }, Back: { value: "A", order: 1 } }, cards: [201] }]);
        case "removeTags": {
          expect(params?.tags).toBe(transactionTag);
          return response(null);
        }
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await add(client, {
      notes: [{ deckName: "技術", modelName: "Basic", fields: { Front: "Q", Back: "A" }, tags: ["quint"] }],
    });
    expect(result.status).toBe("added");
    expect(result.observedNoteIds).toEqual([101]);
    expect(actions).toEqual(["getTags", "canAddNotesWithErrorDetail", "addNotes", "findNotes", "notesInfo", "removeTags"]);
  });

  test("reports only notes observed in Anki when addNotes partially fails", async () => {
    let transactionTag = "";
    const client = fakeClient((action, params) => {
      switch (action) {
        case "getTags": return response(["quint"]);
        case "canAddNotesWithErrorDetail": {
          const notes = params?.notes as Array<{ tags: string[] }>;
          transactionTag = notes[0].tags.find((tag) => tag.startsWith("create-anki-cards-tx-")) ?? "";
          return response([{ canAdd: true, error: null }, { canAdd: true, error: null }]);
        }
        case "addNotes": return response([101, null]);
        case "findNotes": return response([101]);
        case "notesInfo": return response([{ noteId: 101, modelName: "Basic", tags: ["quint", transactionTag], fields: {}, cards: [] }]);
        case "removeTags": return response(null);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await add(client, {
      notes: [
        { deckName: "技術", modelName: "Basic", fields: { Front: "Q1", Back: "A1" }, tags: ["quint"] },
        { deckName: "技術", modelName: "Basic", fields: { Front: "Q2", Back: "A2" }, tags: ["quint"] },
      ],
    });
    expect(result.status).toBe("partial-or-recovered");
    expect(result.returnedFailures).toEqual([1]);
    expect(result.observedNoteIds).toEqual([101]);
  });
});

describe("recover", () => {
  test("finds unfinished transaction tags in Anki and cleans them", async () => {
    const removed: string[] = [];
    const client = fakeClient((action, params) => {
      switch (action) {
        case "getTags": return response(["quint", "create-anki-cards-tx-one"]);
        case "findNotes": return response([101]);
        case "notesInfo": return response([{ noteId: 101, modelName: "Basic", tags: ["quint", "create-anki-cards-tx-one"], fields: {}, cards: [] }]);
        case "removeTags": {
          removed.push(String(params?.tags));
          return response(null);
        }
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await recover(client);
    expect((result.recovered as Array<{ noteIds: number[] }>)[0].noteIds).toEqual([101]);
    expect(removed).toEqual(["create-anki-cards-tx-one"]);
  });
});

describe("input", () => {
  test("rejects empty note lists", () => {
    expect(() => parseAddInput({ notes: [] })).toThrow("non-empty notes array");
  });
});
