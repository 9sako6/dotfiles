#!/usr/bin/env bun

import { AnkiConnectClient } from "./anki-connect";

const GOAL_MODEL_NAME = "Goal";
const GOAL_FIELD_NAME = "Definition";
const NEW_TAG_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*(?:::[a-z0-9]+(?:-[a-z0-9]+)*)*$/u;

type JsonRecord = Record<string, unknown>;

type NoteInfo = {
  noteId: number;
  modelName: string;
  tags: string[];
  fields: Record<string, { value: string; order: number }>;
  cards: number[];
};

type CardInfo = {
  cardId: number;
  note: number;
  deckName: string;
};

type GoalRecord = {
  noteId: number;
  definition: string | null;
  tags: string[];
  cards: Array<{
    cardId: number;
    deckName: string | null;
    suspended: boolean | null;
  }>;
};

type GoalState = {
  status: "missing" | "found" | "conflict";
  tag: string;
  goals: GoalRecord[];
};

function requireTag(tag: string): string {
  if (tag.length === 0 || /\s/u.test(tag)) {
    throw new Error("tag must be a non-empty Anki tag without whitespace");
  }
  return tag;
}

async function readGoalRecords(client: AnkiConnectClient, tag: string): Promise<GoalRecord[]> {
  const noteIds = await client.invoke<number[]>("findNotes", {
    query: `tag:${requireTag(tag)} note:${GOAL_MODEL_NAME}`,
  });
  if (noteIds.length === 0) {
    return [];
  }

  const notes = await client.invoke<NoteInfo[]>("notesInfo", { notes: noteIds });
  const cardIds = [...new Set(notes.flatMap((note) => note.cards))];
  const cardInfos = cardIds.length === 0
    ? []
    : await client.invoke<CardInfo[]>("cardsInfo", { cards: cardIds });
  const suspended = cardIds.length === 0
    ? []
    : await client.invoke<Array<boolean | null>>("areSuspended", { cards: cardIds });
  const cardsById = new Map(cardInfos.map((card) => [card.cardId, card]));
  const suspendedById = new Map(cardIds.map((cardId, index) => [cardId, suspended[index] ?? null]));

  return notes.map((note) => ({
    noteId: note.noteId,
    definition: note.fields[GOAL_FIELD_NAME]?.value ?? null,
    tags: note.tags,
    cards: note.cards.map((cardId) => ({
      cardId,
      deckName: cardsById.get(cardId)?.deckName ?? null,
      suspended: suspendedById.get(cardId) ?? null,
    })),
  }));
}

export async function getGoal(client: AnkiConnectClient, tag: string): Promise<GoalState> {
  const goals = await readGoalRecords(client, tag);
  return {
    status: goals.length === 0 ? "missing" : goals.length === 1 ? "found" : "conflict",
    tag,
    goals,
  };
}

async function ensureTopicTag(client: AnkiConnectClient, tag: string): Promise<void> {
  requireTag(tag);
  const existing = new Set(await client.invoke<string[]>("getTags"));
  if (!existing.has(tag) && !NEW_TAG_PATTERN.test(tag)) {
    throw new Error(
      `new tag ${JSON.stringify(tag)} must use lowercase letters, digits, hyphens, and :: only`,
    );
  }
}

async function ensureGoalModel(client: AnkiConnectClient): Promise<{ created: boolean }> {
  const modelNames = await client.invoke<string[]>("modelNames");
  let created = false;
  if (!modelNames.includes(GOAL_MODEL_NAME)) {
    let createError: string | null = null;
    try {
      await client.invoke("createModel", {
        modelName: GOAL_MODEL_NAME,
        inOrderFields: [GOAL_FIELD_NAME],
        cardTemplates: [
          {
            Name: GOAL_MODEL_NAME,
            Front: `{{${GOAL_FIELD_NAME}}}`,
            Back: `{{${GOAL_FIELD_NAME}}}`,
          },
        ],
      });
    } catch (error) {
      createError = error instanceof Error ? error.message : String(error);
    }

    const afterCreate = await client.invoke<string[]>("modelNames");
    if (!afterCreate.includes(GOAL_MODEL_NAME)) {
      throw new Error(
        createError === null
          ? "Goal note type was not created"
          : `Goal note type creation failed: ${createError}`,
      );
    }
    created = true;
  }

  const fields = await client.invoke<string[]>("modelFieldNames", { modelName: GOAL_MODEL_NAME });
  if (fields.length !== 1 || fields[0] !== GOAL_FIELD_NAME) {
    throw new Error(
      `existing Goal note type is incompatible: expected only ${GOAL_FIELD_NAME}, got ${fields.join(", ")}`,
    );
  }
  return { created };
}

async function ensureCardsSuspended(
  client: AnkiConnectClient,
  cards: number[],
): Promise<Array<boolean | null>> {
  if (cards.length === 0) {
    throw new Error("Goal note must generate at least one card");
  }

  const before = await client.invoke<Array<boolean | null>>("areSuspended", { cards });
  const toSuspend = cards.filter((_, index) => before[index] === false);
  if (toSuspend.length > 0) {
    await client.invoke<boolean>("suspend", { cards: toSuspend });
  }
  const after = await client.invoke<Array<boolean | null>>("areSuspended", { cards });
  if (after.some((value) => value !== true)) {
    throw new Error("Goal card suspension could not be verified");
  }
  return after;
}

function normalizeDefinition(value: string): string {
  const definition = value.trim();
  if (definition.length === 0) {
    throw new Error("Definition must not be empty");
  }
  return definition;
}

async function verifySingleGoal(
  client: AnkiConnectClient,
  tag: string,
  definition: string,
): Promise<GoalRecord> {
  const state = await getGoal(client, tag);
  if (state.status !== "found") {
    throw new Error(`Goal verification failed: expected one Goal, got ${state.goals.length}`);
  }
  const goal = state.goals[0];
  if (goal.definition !== definition) {
    throw new Error("Goal verification failed: Definition does not match the approved value");
  }
  await ensureCardsSuspended(client, goal.cards.map((card) => card.cardId));
  return (await getGoal(client, tag)).goals[0];
}

export async function setGoal(
  client: AnkiConnectClient,
  tag: string,
  deckName: string,
  rawDefinition: string,
): Promise<JsonRecord> {
  const definition = normalizeDefinition(rawDefinition);
  const current = await getGoal(client, tag);
  if (current.status === "conflict") {
    return { status: "conflict", tag, goals: current.goals };
  }

  if (current.status === "found") {
    const existing = current.goals[0];
    if (existing.definition === definition) {
      await ensureCardsSuspended(client, existing.cards.map((card) => card.cardId));
      return { status: "unchanged", tag, goal: (await getGoal(client, tag)).goals[0] };
    }

    let updateError: string | null = null;
    try {
      await client.invoke("updateNoteFields", {
        note: {
          id: existing.noteId,
          fields: { [GOAL_FIELD_NAME]: definition },
        },
      });
    } catch (error) {
      updateError = error instanceof Error ? error.message : String(error);
    }

    const goal = await verifySingleGoal(client, tag, definition);
    return {
      status: updateError === null ? "updated" : "updated-after-uncertain-response",
      tag,
      goal,
      updateError,
    };
  }

  await ensureTopicTag(client, tag);
  const decks = await client.invoke<string[]>("deckNames");
  if (!decks.includes(deckName)) {
    throw new Error(`deck does not exist: ${deckName}`);
  }
  const model = await ensureGoalModel(client);

  let addError: string | null = null;
  try {
    await client.invoke<number | null>("addNote", {
      note: {
        deckName,
        modelName: GOAL_MODEL_NAME,
        fields: { [GOAL_FIELD_NAME]: definition },
        tags: [tag],
        options: { allowDuplicate: true },
      },
    });
  } catch (error) {
    addError = error instanceof Error ? error.message : String(error);
  }

  const observed = await getGoal(client, tag);
  if (observed.status === "missing") {
    throw new Error(
      addError === null
        ? "Goal creation could not be verified"
        : `Goal creation failed and no Goal was observed: ${addError}`,
    );
  }
  if (observed.status === "conflict") {
    return { status: "conflict-after-write", tag, goals: observed.goals, addError };
  }
  if (observed.goals[0].definition !== definition) {
    return { status: "concurrent-goal", tag, goal: observed.goals[0], addError };
  }

  const goal = await verifySingleGoal(client, tag, definition);
  return {
    status: addError === null ? "created" : "created-after-uncertain-response",
    tag,
    goal,
    noteTypeCreated: model.created,
    addError,
  };
}

async function readStdinText(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  if (chunks.length === 0) {
    throw new Error("stdin is empty");
  }
  return Buffer.concat(chunks).toString("utf8");
}

function usage(): never {
  throw new Error(
    [
      "usage:",
      "  goal.ts get TAG",
      "  goal.ts set TAG DECK_NAME < definition.txt",
    ].join("\n"),
  );
}

export async function main(argv = process.argv.slice(2)): Promise<void> {
  const client = new AnkiConnectClient();
  const [command, ...args] = argv;
  let result: JsonRecord;
  switch (command) {
    case "get":
      if (args.length !== 1) usage();
      result = await getGoal(client, args[0]);
      break;
    case "set":
      if (args.length !== 2) usage();
      result = await setGoal(client, args[0], args[1], await readStdinText());
      break;
    default:
      usage();
  }
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
}

if (import.meta.main) {
  main().catch((error) => {
    const message = error instanceof Error ? error.message : String(error);
    process.stderr.write(`${message}\n`);
    process.exitCode = 1;
  });
}
