import { describe, expect, test } from "bun:test";

import {
  AnkiConnectClient,
  add,
  assertLocalEndpoint,
  exactTagQuery,
  parseAddInput,
  snapshot,
} from "./anki-connect.ts";

const PENDING_TRANSACTION_QUERY = "tag:re:^anki-tx-.*$";

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

describe("exact tags", () => {
  test("uses Anki's exact-tag regular expression search", () => {
    expect(exactTagQuery("quint")).toBe("tag:re:^quint$");
    expect(exactTagQuery("c++")).toBe("tag:re:^c\\+\\+$");
  });
});

describe("snapshot", () => {
  test("reads notes by exact tag and hides internal transaction tags", async () => {
    const client = fakeClient((action, params) => {
      switch (action) {
        case "version": return response(6);
        case "deckNames": return response(["技術"]);
        case "getTags": return response(["quint", "quint::syntax", "anki-tx-stale"]);
        case "modelNames": return response(["Basic", "Goal"]);
        case "findNotes": {
          expect(params?.query).toBe("tag:re:^quint$");
          return response([10]);
        }
        case "notesInfo": return response([{ noteId: 10, modelName: "Basic", tags: ["quint"], fields: { Front: { value: "Q", order: 0 }, Back: { value: "A", order: 1 } }, cards: [20] }]);
        case "cardsInfo": return response([{ cardId: 20, note: 10, deckName: "技術" }]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await snapshot(client, "quint");
    expect(result.decks).toEqual(["技術"]);
    expect(result.tags).toEqual(["quint", "quint::syntax"]);
    expect(result.tag).toBe("quint");
    expect((result.notes as unknown[]).length).toBe(1);
    expect((result.cards as unknown[]).length).toBe(1);
    expect("pendingTransactions" in result).toBe(false);
  });
});

describe("add", () => {
  test("does not impose a custom naming convention on Anki tags", async () => {
    const client = fakeClient((action, params) => {
      switch (action) {
        case "findNotes":
          expect(params?.query).toBe(PENDING_TRANSACTION_QUERY);
          return response([]);
        case "canAddNotesWithErrorDetail": return response([{ canAdd: false, error: "stop" }]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await add(client, {
      notes: [{ deckName: "技術", modelName: "Basic", fields: { Front: "Q", Back: "A" }, tags: ["日本語"] }],
    });
    expect(result.status).toBe("rejected");
  });

  test("rejects the internal transaction namespace", async () => {
    const client = fakeClient(() => {
      throw new Error("AnkiConnect must not be called");
    });
    await expect(add(client, {
      notes: [{ deckName: "技術", modelName: "Basic", fields: { Front: "Q", Back: "A" }, tags: ["anki-tx-user"] }],
    })).rejects.toThrow("reserved internal tag prefix");
  });

  test("reconciles a stale write without starting the current write", async () => {
    const actions: string[] = [];
    const client = fakeClient((action, params) => {
      actions.push(action);
      switch (action) {
        case "findNotes": {
          expect(params?.query).toBe(PENDING_TRANSACTION_QUERY);
          return response([90]);
        }
        case "notesInfo": return response([
          { noteId: 90, modelName: "Basic", tags: ["quint", "anki-tx-stale"], fields: {}, cards: [] },
        ]);
        case "removeTags": {
          expect(params?.notes).toEqual([90]);
          expect(params?.tags).toBe("anki-tx-stale");
          return response(null);
        }
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await add(client, {
      notes: [{ deckName: "技術", modelName: "Basic", fields: { Front: "Q", Back: "A" }, tags: ["quint"] }],
    });
    expect(result.status).toBe("rejected");
    expect(result.status === "rejected" ? result.errors[0].error : "").toContain("Read Anki again");
    expect(actions).not.toContain("canAddNotesWithErrorDetail");
    expect(actions).not.toContain("addNotes");
    expect(actions).not.toContain("getTags");
  });

  test("ignores unused internal tags that are not attached to notes", async () => {
    let transactionTag = "";
    const client = fakeClient((action, params) => {
      switch (action) {
        case "findNotes": {
          const query = String(params?.query);
          if (query === PENDING_TRANSACTION_QUERY) return response([]);
          expect(query).toBe(`tag:re:^${transactionTag}$`);
          return response([101]);
        }
        case "canAddNotesWithErrorDetail": {
          const notes = params?.notes as Array<{ tags: string[] }>;
          transactionTag = notes[0].tags.find((tag) => tag.startsWith("anki-tx-")) ?? "";
          return response([{ canAdd: true, error: null }]);
        }
        case "addNotes": return response([101]);
        case "notesInfo": return response([
          { noteId: 101, modelName: "Basic", tags: ["quint", transactionTag], fields: { Front: { value: "Q", order: 0 }, Back: { value: "A", order: 1 } }, cards: [201] },
        ]);
        case "removeTags": return response(null);
        case "getTags": throw new Error("tag registry must not decide pending writes");
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await add(client, {
      notes: [{ deckName: "技術", modelName: "Basic", fields: { Front: "Q", Back: "A" }, tags: ["quint"] }],
    });
    expect(result).toEqual({ status: "success", noteIds: [101] });
  });

  test("returns indeterminate when stale write reconciliation fails", async () => {
    const actions: string[] = [];
    const client = fakeClient((action, params) => {
      actions.push(action);
      switch (action) {
        case "findNotes": {
          expect(params?.query).toBe(PENDING_TRANSACTION_QUERY);
          return response([90]);
        }
        case "notesInfo": return response([
          { noteId: 90, modelName: "Basic", tags: ["quint", "anki-tx-stale"], fields: {}, cards: [] },
        ]);
        case "removeTags": throw new Error("cleanup failed");
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await add(client, {
      notes: [{ deckName: "技術", modelName: "Basic", fields: { Front: "Q", Back: "A" }, tags: ["quint"] }],
    });
    expect(result.status).toBe("indeterminate");
    expect(actions).not.toContain("canAddNotesWithErrorDetail");
    expect(actions).not.toContain("addNotes");
  });

  test("adds normally and returns only the public result", async () => {
    let transactionTag = "";
    const client = fakeClient((action, params) => {
      switch (action) {
        case "findNotes": {
          const query = String(params?.query);
          if (query === PENDING_TRANSACTION_QUERY) return response([]);
          expect(query).toBe(`tag:re:^${transactionTag}$`);
          return response([101]);
        }
        case "canAddNotesWithErrorDetail": {
          const notes = params?.notes as Array<{ tags: string[] }>;
          transactionTag = notes[0].tags.find((tag) => tag.startsWith("anki-tx-")) ?? "";
          return response([{ canAdd: true, error: null }]);
        }
        case "addNotes": return response([101]);
        case "notesInfo": return response([
          { noteId: 101, modelName: "Basic", tags: ["quint", transactionTag], fields: { Front: { value: "Q", order: 0 }, Back: { value: "A", order: 1 } }, cards: [201] },
        ]);
        case "removeTags": return response(null);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await add(client, {
      notes: [{ deckName: "技術", modelName: "Basic", fields: { Front: "Q", Back: "A" }, tags: ["quint"] }],
    });
    expect(result).toEqual({ status: "success", noteIds: [101] });
    expect("transactionTag" in result).toBe(false);
  });

  test("reports a verified partial write without exposing transaction details", async () => {
    let transactionTag = "";
    const client = fakeClient((action, params) => {
      switch (action) {
        case "findNotes": {
          const query = String(params?.query);
          if (query === PENDING_TRANSACTION_QUERY) return response([]);
          return response([101]);
        }
        case "canAddNotesWithErrorDetail": {
          const notes = params?.notes as Array<{ tags: string[] }>;
          transactionTag = notes[0].tags.find((tag) => tag.startsWith("anki-tx-")) ?? "";
          return response([{ canAdd: true, error: null }, { canAdd: true, error: null }]);
        }
        case "addNotes": return response([101, null]);
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
    expect(result).toEqual({ status: "partial", noteIds: [101], missing: 1 });
  });

  test("returns indeterminate when Anki cannot verify a write", async () => {
    const client = fakeClient((action, params) => {
      switch (action) {
        case "findNotes": {
          if (params?.query === PENDING_TRANSACTION_QUERY) return response([]);
          throw new Error("connection lost");
        }
        case "canAddNotesWithErrorDetail": return response([{ canAdd: true, error: null }]);
        case "addNotes": return response([101]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await add(client, {
      notes: [{ deckName: "技術", modelName: "Basic", fields: { Front: "Q", Back: "A" }, tags: ["quint"] }],
    });
    expect(result.status).toBe("indeterminate");
    expect("transactionTag" in result).toBe(false);
  });
});

describe("input", () => {
  test("rejects empty note lists", () => {
    expect(() => parseAddInput({ notes: [] })).toThrow("non-empty notes array");
  });
});
