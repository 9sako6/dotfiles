#!/usr/bin/env bun

import {
  AnkiConnectClient,
  type CardInfo,
  type NoteInfo,
  notesWithExactTag,
} from "./anki-connect.ts";

const GOAL_MODEL_NAME = "Goal";
const GOAL_FIELD_NAME = "Definition";

type JsonRecord = Record<string, unknown>;

type GoalRecord = {
  noteId: number;
  definition: string;
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

function requireGoalFieldNames(fieldNames: string[], subject: string): void {
  if (fieldNames.length !== 1 || fieldNames[0] !== GOAL_FIELD_NAME) {
    throw new Error(
      `${subject} is incompatible: expected only ${GOAL_FIELD_NAME}, got ${fieldNames.join(", ")}`,
    );
  }
}

function definitionFromGoalNote(note: NoteInfo): string {
  requireGoalFieldNames(Object.keys(note.fields), `Goal note ${note.noteId}`);
  const definition = note.fields[GOAL_FIELD_NAME].value;
  if (definition.trim().length === 0) {
    throw new Error(`Goal note ${note.noteId} has an empty ${GOAL_FIELD_NAME}`);
  }
  if (note.cards.length === 0) {
    throw new Error(`Goal note ${note.noteId} does not generate a card`);
  }
  return definition;
}

async function readGoalRecords(client: AnkiConnectClient, tag: string): Promise<GoalRecord[]> {
  const goalNotes = (await notesWithExactTag(client, tag))
    .filter((note) => note.modelName === GOAL_MODEL_NAME)
    .map((note) => ({ note, definition: definitionFromGoalNote(note) }));
  if (goalNotes.length === 0) {
    return [];
  }

  const cardIds = [...new Set(goalNotes.flatMap(({ note }) => note.cards))];
  const [cardInfos, suspended] = await Promise.all([
    client.invoke<CardInfo[]>("cardsInfo", { cards: cardIds }),
    client.invoke<Array<boolean | null>>("areSuspended", { cards: cardIds }),
  ]);
  if (cardInfos.length !== cardIds.length || suspended.length !== cardIds.length) {
    throw new Error("Goal card state could not be read completely");
  }

  const cardsById = new Map(cardInfos.map((card) => [card.cardId, card]));
  const suspendedById = new Map(cardIds.map((cardId, index) => [cardId, suspended[index] ?? null]));

  return goalNotes.map(({ note, definition }) => ({
    noteId: note.noteId,
    definition,
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
  requireGoalFieldNames(fields, "existing Goal note type");
  return { created };
}

async function ensureCardsSuspended(client: AnkiConnectClient, cards: number[]): Promise<void> {
  if (cards.length === 0) {
    throw new Error("Goal note must generate at least one card");
  }

  const before = await client.invoke<Array<boolean | null>>("areSuspended", { cards });
  if (before.length !== cards.length) {
    throw new Error("Goal card suspension state could not be read completely");
  }
  const toSuspend = cards.filter((_, index) => before[index] === false);
  if (toSuspend.length > 0) {
    await client.invoke<boolean>("suspend", { cards: toSuspend });
  }
  const after = await client.invoke<Array<boolean | null>>("areSuspended", { cards });
  if (after.length !== cards.length || after.some((value) => value !== true)) {
    throw new Error("Goal card suspension could not be verified");
  }
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

async function deckCandidatesForTag(client: AnkiConnectClient, tag: string): Promise<string[]> {
  const notes = (await notesWithExactTag(client, tag)).filter((note) => note.modelName !== GOAL_MODEL_NAME);
  const cardIds = [...new Set(notes.flatMap((note) => note.cards))];
  if (cardIds.length === 0) {
    return [];
  }
  const cards = await client.invoke<CardInfo[]>("cardsInfo", { cards: cardIds });
  return [...new Set(cards.map((card) => card.deckName))].sort();
}

async function resolveGoalDeck(
  client: AnkiConnectClient,
  tag: string,
  requestedDeck?: string,
): Promise<string | { status: "needs-deck"; tag: string; candidateDecks: string[] }> {
  if (requestedDeck !== undefined) {
    const decks = await client.invoke<string[]>("deckNames");
    if (!decks.includes(requestedDeck)) {
      throw new Error(`deck does not exist: ${requestedDeck}`);
    }
    return requestedDeck;
  }

  const candidateDecks = await deckCandidatesForTag(client, tag);
  return candidateDecks.length === 1
    ? candidateDecks[0]
    : { status: "needs-deck", tag, candidateDecks };
}

export async function setGoal(
  client: AnkiConnectClient,
  tag: string,
  rawDefinition: string,
  deckName?: string,
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

    try {
      await client.invoke("updateNoteFields", {
        note: {
          id: existing.noteId,
          fields: { [GOAL_FIELD_NAME]: definition },
        },
      });
    } catch {
      // Verify from Anki below; a transport error does not imply that the update failed.
    }

    return { status: "updated", tag, goal: await verifySingleGoal(client, tag, definition) };
  }

  const resolvedDeck = await resolveGoalDeck(client, tag, deckName);
  if (typeof resolvedDeck !== "string") {
    return resolvedDeck;
  }
  const model = await ensureGoalModel(client);

  try {
    await client.invoke<number | null>("addNote", {
      note: {
        deckName: resolvedDeck,
        modelName: GOAL_MODEL_NAME,
        fields: { [GOAL_FIELD_NAME]: definition },
        tags: [tag],
        options: { allowDuplicate: true },
      },
    });
  } catch {
    // Verify from Anki below; a transport error does not imply that the add failed.
  }

  const observed = await getGoal(client, tag);
  if (observed.status === "missing") {
    throw new Error("Goal creation could not be verified");
  }
  if (observed.status === "conflict") {
    return { status: "conflict", tag, goals: observed.goals };
  }
  if (observed.goals[0].definition !== definition) {
    return { status: "conflict", tag, goals: observed.goals };
  }

  return {
    status: "created",
    tag,
    goal: await verifySingleGoal(client, tag, definition),
    noteTypeCreated: model.created,
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
      "  goal.ts set TAG [DECK_NAME] < definition.txt",
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
      if (args.length < 1 || args.length > 2) usage();
      result = await setGoal(client, args[0], await readStdinText(), args[1]);
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
