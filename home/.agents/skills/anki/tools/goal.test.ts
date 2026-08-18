import { describe, expect, test } from "bun:test";

import { AnkiConnectClient } from "./anki-connect.ts";
import { getGoal, setGoal } from "./goal.ts";

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

function learningNote(noteId: number, cardId: number) {
  return {
    noteId,
    modelName: "Basic",
    tags: ["quint"],
    fields: { Front: { value: "Q", order: 0 }, Back: { value: "A", order: 1 } },
    cards: [cardId],
  };
}

describe("getGoal", () => {
  test("resolves Goal through an exact Anki tag search", async () => {
    const client = fakeClient((action, params) => {
      switch (action) {
        case "findNotes":
          expect(params?.query).toBe("tag:re:^quint$");
          return response([10]);
        case "notesInfo": return response([goalNote(10, "Quintで仕様を検証できる", 20)]);
        case "cardsInfo": return response([{ cardId: 20, note: 10, deckName: "技術" }]);
        case "areSuspended": return response([true]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await getGoal(client, "quint");
    expect(result.status).toBe("found");
    expect(result.goals[0].definition).toBe("Quintで仕様を検証できる");
  });

  test("rejects a Goal note with an incompatible field structure", async () => {
    const client = fakeClient((action) => {
      switch (action) {
        case "findNotes": return response([10]);
        case "notesInfo": return response([{
          ...goalNote(10, "Goal", 20),
          fields: { Objective: { value: "Goal", order: 0 } },
        }]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    await expect(getGoal(client, "quint")).rejects.toThrow("expected only Definition");
  });

  test("rejects a Goal note with an empty Definition", async () => {
    const client = fakeClient((action) => {
      switch (action) {
        case "findNotes": return response([10]);
        case "notesInfo": return response([goalNote(10, "   ", 20)]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    await expect(getGoal(client, "quint")).rejects.toThrow("empty Definition");
  });

  test("rejects a Goal note that does not generate a card", async () => {
    const client = fakeClient((action) => {
      switch (action) {
        case "findNotes": return response([10]);
        case "notesInfo": return response([{ ...goalNote(10, "Goal", 20), cards: [] }]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    await expect(getGoal(client, "quint")).rejects.toThrow("does not generate a card");
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
  test("infers the deck from existing learning cards when creating a Goal", async () => {
    let goalExists = false;
    let modelExists = false;
    let suspended = false;
    let definition = "";

    const client = fakeClient((action, params) => {
      switch (action) {
        case "findNotes": return response(goalExists ? [1, 10] : [1]);
        case "notesInfo": {
          const ids = params?.notes as number[];
          return response(ids.map((id) => id === 10 ? goalNote(10, definition, 20) : learningNote(1, 11)));
        }
        case "cardsInfo": {
          const ids = params?.cards as number[];
          return response(ids.map((id) => id === 20
            ? { cardId: 20, note: 10, deckName: "技術" }
            : { cardId: 11, note: 1, deckName: "技術" }));
        }
        case "areSuspended": return response([suspended]);
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
            deckName: string;
            modelName: string;
            fields: Record<string, string>;
            tags: string[];
          };
          expect(note.deckName).toBe("技術");
          expect(note.modelName).toBe("Goal");
          expect(note.tags).toEqual(["quint"]);
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

    const result = await setGoal(client, "quint", "  Quintで仕様を検証できる\n");
    expect(result.status).toBe("created");
    expect(result.noteTypeCreated).toBe(true);
    expect(definition).toBe("Quintで仕様を検証できる");
    expect(suspended).toBe(true);
  });

  test("asks for a deck only when a new Goal cannot infer one", async () => {
    const actions: string[] = [];
    const client = fakeClient((action) => {
      actions.push(action);
      switch (action) {
        case "findNotes": return response([]);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await setGoal(client, "quint", "新しいゴール");
    expect(result).toEqual({ status: "needs-deck", tag: "quint", candidateDecks: [] });
    expect(actions).not.toContain("modelNames");
    expect(actions).not.toContain("addNote");
  });

  test("accepts an explicitly chosen deck when inference is impossible", async () => {
    let goalExists = false;
    const client = fakeClient((action, params) => {
      switch (action) {
        case "findNotes": return response(goalExists ? [10] : []);
        case "notesInfo": return response([goalNote(10, "新しいゴール", 20)]);
        case "cardsInfo": return response([{ cardId: 20, note: 10, deckName: "技術" }]);
        case "areSuspended": return response([true]);
        case "deckNames": return response(["技術", "語学"]);
        case "modelNames": return response(["Goal"]);
        case "modelFieldNames": return response(["Definition"]);
        case "addNote": {
          const note = params?.note as { deckName: string };
          expect(note.deckName).toBe("技術");
          goalExists = true;
          return response(10);
        }
        case "suspend": return response(true);
        default: throw new Error(`unexpected action ${action}`);
      }
    });

    const result = await setGoal(client, "quint", "新しいゴール", "技術");
    expect(result.status).toBe("created");
  });

  test("updates Definition without requiring or resolving a deck", async () => {
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

    const result = await setGoal(client, "quint", "新しいゴール");
    expect(result.status).toBe("updated");
    expect(definition).toBe("新しいゴール");
    expect(suspended).toBe(true);
    expect(actions).not.toContain("deckNames");
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

    const result = await setGoal(client, "quint", "新しいゴール");
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

    const result = await setGoal(client, "quint", "同じゴール");
    expect(result.status).toBe("unchanged");
    expect(actions).not.toContain("addNote");
    expect(actions).not.toContain("updateNoteFields");
  });
});
