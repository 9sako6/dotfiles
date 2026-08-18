import { describe, expect, test } from "bun:test";

import { AnkiConnectClient } from "./anki-connect";
import { getGoal, setGoal } from "./goal";

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

function goalNote(noteId: number, definition: string, cardId: number) {
  return {
    noteId,
    modelName: "Goal",
    tags: ["quint"],
    fields: { Definition: { value: definition, order: 0 } },
    cards: [cardId],
  };
}

describe("getGoal", () => {
  test("returns the single Goal and its suspension state", async () => {
    const client = fakeClient((action) => {
      switch (action) {
        case "findNotes": return response([10]);
        case "notesInfo": return response([goalNote(10, "Quintで仕様を検証できる", 20)]);
        case "cardsInfo": return response([{ cardId: 20, note: 10, deckName: "技術" }]);
        case "areSuspended": return response([true]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await getGoal(client, "quint");
    expect(result.status).toBe("found");
    expect(result.goals[0].definition).toBe("Quintで仕様を検証できる");
    expect(result.goals[0].cards).toEqual([
      { cardId: 20, deckName: "技術", suspended: true },
    ]);
  });

  test("reports multiple Goal notes as a conflict", async () => {
    const client = fakeClient((action) => {
      switch (action) {
        case "findNotes": return response([10, 11]);
        case "notesInfo": return response([
          goalNote(10, "Goal A", 20),
          goalNote(11, "Goal B", 21),
        ]);
        case "cardsInfo": return response([
          { cardId: 20, note: 10, deckName: "技術" },
          { cardId: 21, note: 11, deckName: "技術" },
        ]);
        case "areSuspended": return response([true, true]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await getGoal(client, "quint");
    expect(result.status).toBe("conflict");
    expect(result.goals).toHaveLength(2);
  });
});

describe("setGoal", () => {
  test("creates Goal/Definition once and suspends the generated card", async () => {
    let goalExists = false;
    let modelExists = false;
    let suspended = false;
    let definition = "";
    const actions: string[] = [];

    const client = fakeClient((action, params) => {
      actions.push(action);
      switch (action) {
        case "findNotes": return response(goalExists ? [10] : []);
        case "notesInfo": return response([goalNote(10, definition, 20)]);
        case "cardsInfo": return response([{ cardId: 20, note: 10, deckName: "技術" }]);
        case "areSuspended": return response([suspended]);
        case "getTags": return response([]);
        case "deckNames": return response(["技術"]);
        case "modelNames": return response(modelExists ? ["Goal"] : []);
        case "createModel": {
          expect(params).toEqual({
            modelName: "Goal",
            inOrderFields: ["Definition"],
            cardTemplates: [
              { Name: "Goal", Front: "{{Definition}}", Back: "{{Definition}}" },
            ],
          });
          modelExists = true;
          return response({});
        }
        case "modelFieldNames": return response(["Definition"]);
        case "addNote": {
          const note = params?.note as {
            modelName: string;
            fields: Record<string, string>;
            tags: string[];
            options: { allowDuplicate: boolean };
          };
          expect(note.modelName).toBe("Goal");
          expect(note.tags).toEqual(["quint"]);
          expect(note.options).toEqual({ allowDuplicate: true });
          definition = note.fields.Definition;
          goalExists = true;
          return response(10);
        }
        case "suspend":
          suspended = true;
          return response(true);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await setGoal(client, "quint", "技術", "  Quintで仕様を検証できる\n");
    expect(result.status).toBe("created");
    expect(result.noteTypeCreated).toBe(true);
    expect(definition).toBe("Quintで仕様を検証できる");
    expect(suspended).toBe(true);
    expect(actions.filter((action) => action === "addNote")).toHaveLength(1);
  });

  test("updates the existing Definition without creating a second Goal", async () => {
    let definition = "古いゴール";
    let suspended = false;
    const actions: string[] = [];

    const client = fakeClient((action, params) => {
      actions.push(action);
      switch (action) {
        case "findNotes": return response([10]);
        case "notesInfo": return response([goalNote(10, definition, 20)]);
        case "cardsInfo": return response([{ cardId: 20, note: 10, deckName: "技術" }]);
        case "areSuspended": return response([suspended]);
        case "updateNoteFields": {
          const note = params?.note as { id: number; fields: Record<string, string> };
          expect(note.id).toBe(10);
          definition = note.fields.Definition;
          return response(null);
        }
        case "suspend":
          suspended = true;
          return response(true);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await setGoal(client, "quint", "技術", "新しいゴール");
    expect(result.status).toBe("updated");
    expect(definition).toBe("新しいゴール");
    expect(suspended).toBe(true);
    expect(actions).not.toContain("addNote");
  });

  test("does not mutate when more than one Goal exists", async () => {
    const actions: string[] = [];
    const client = fakeClient((action) => {
      actions.push(action);
      switch (action) {
        case "findNotes": return response([10, 11]);
        case "notesInfo": return response([
          goalNote(10, "Goal A", 20),
          goalNote(11, "Goal B", 21),
        ]);
        case "cardsInfo": return response([
          { cardId: 20, note: 10, deckName: "技術" },
          { cardId: 21, note: 11, deckName: "技術" },
        ]);
        case "areSuspended": return response([true, true]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await setGoal(client, "quint", "技術", "新しいゴール");
    expect(result.status).toBe("conflict");
    expect(actions).not.toContain("addNote");
    expect(actions).not.toContain("updateNoteFields");
  });

  test("reuses the same Goal when Definition is unchanged", async () => {
    const actions: string[] = [];
    const client = fakeClient((action) => {
      actions.push(action);
      switch (action) {
        case "findNotes": return response([10]);
        case "notesInfo": return response([goalNote(10, "同じゴール", 20)]);
        case "cardsInfo": return response([{ cardId: 20, note: 10, deckName: "技術" }]);
        case "areSuspended": return response([true]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await setGoal(client, "quint", "技術", "同じゴール");
    expect(result.status).toBe("unchanged");
    expect(actions).not.toContain("addNote");
    expect(actions).not.toContain("updateNoteFields");
  });
});
