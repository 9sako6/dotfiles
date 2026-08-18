import { describe, expect, test } from "bun:test";

import {
  AnkiConnectClient,
  add,
  snapshot,
} from "./anki-connect.ts";

function response(result: unknown, error: string | null = null): Response {
  return new Response(JSON.stringify({ result, error }), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

function fakeClient(
  handler: (action: string, params: Record<string, unknown> | undefined) => Response,
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

describe("public snapshot", () => {
  test("hides internal transaction tags from collection and note metadata", async () => {
    const client = fakeClient((action) => {
      switch (action) {
        case "version": return response(6);
        case "deckNames": return response(["技術"]);
        case "getTags": return response(["quint", "anki-tx-stale"]);
        case "modelNames": return response(["Basic"]);
        case "findNotes": return response([10]);
        case "notesInfo": return response([{
          noteId: 10,
          modelName: "Basic",
          tags: ["quint", "anki-tx-stale"],
          fields: {},
          cards: [],
        }]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await snapshot(client, "quint");
    expect(result.tags).toEqual(["quint"]);
    expect((result.notes as Array<{ tags: string[] }>)[0].tags).toEqual(["quint"]);
  });
});

describe("preflight boundary", () => {
  test("does not write when Anki returns a result count different from the request", async () => {
    const actions: string[] = [];
    const client = fakeClient((action) => {
      actions.push(action);
      switch (action) {
        case "findNotes": return response([]);
        case "canAddNotesWithErrorDetail": return response([]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await add(client, {
      notes: [{
        deckName: "技術",
        modelName: "Basic",
        fields: { Front: "Q", Back: "A" },
        tags: ["quint"],
      }],
    });

    expect(result.status).toBe("rejected");
    expect(actions).not.toContain("addNotes");
  });
});
