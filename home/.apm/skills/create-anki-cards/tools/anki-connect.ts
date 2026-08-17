#!/usr/bin/env bun

import { randomUUID } from "node:crypto";

const API_VERSION = 6;
const DEFAULT_ENDPOINT = "http://127.0.0.1:8765";
const TRANSACTION_TAG_PREFIX = "create-anki-cards-tx-";
const NEW_TAG_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*(?:::[a-z0-9]+(?:-[a-z0-9]+)*)*$/u;

type JsonRecord = Record<string, unknown>;

type NoteInput = {
  deckName: string;
  modelName: string;
  fields: Record<string, string>;
  tags: string[];
};

type AddInput = {
  notes: NoteInput[];
};

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
  modelName?: string;
  question?: string;
  answer?: string;
};

type CanAddResult = {
  canAdd: boolean;
  error: string | null;
};

type FetchLike = typeof fetch;

export class AnkiConnectClient {
  readonly endpoint: string;
  readonly fetchImpl: FetchLike;

  constructor(endpoint = process.env.ANKI_CONNECT_URL ?? DEFAULT_ENDPOINT, fetchImpl: FetchLike = fetch) {
    assertLocalEndpoint(endpoint);
    this.endpoint = endpoint;
    this.fetchImpl = fetchImpl;
  }

  async invoke<T>(action: string, params?: JsonRecord): Promise<T> {
    const response = await this.fetchImpl(this.endpoint, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ action, version: API_VERSION, ...(params === undefined ? {} : { params }) }),
    });
    if (!response.ok) {
      throw new Error(`${action}: HTTP ${response.status}`);
    }
    const body = (await response.json()) as { result: T; error: string | null };
    if (body.error !== null) {
      throw new Error(`${action}: ${body.error}`);
    }
    return body.result;
  }
}

export function assertLocalEndpoint(endpoint: string): void {
  const url = new URL(endpoint);
  if (url.protocol !== "http:") {
    throw new Error("AnkiConnect endpoint must use http");
  }
  if (!["127.0.0.1", "localhost", "[::1]"].includes(url.hostname)) {
    throw new Error("AnkiConnect endpoint must be loopback-only");
  }
  if (url.username !== "" || url.password !== "") {
    throw new Error("AnkiConnect endpoint must not include credentials");
  }
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parseAddInput(value: unknown): AddInput {
  if (!isRecord(value) || !Array.isArray(value.notes) || value.notes.length === 0) {
    throw new Error("stdin must be an object containing a non-empty notes array");
  }
  const notes = value.notes.map((raw, index): NoteInput => {
    if (!isRecord(raw)) {
      throw new Error(`notes[${index}] must be an object`);
    }
    const { deckName, modelName, fields, tags } = raw;
    if (typeof deckName !== "string" || deckName.length === 0) {
      throw new Error(`notes[${index}].deckName must be a non-empty string`);
    }
    if (typeof modelName !== "string" || modelName.length === 0) {
      throw new Error(`notes[${index}].modelName must be a non-empty string`);
    }
    if (!isRecord(fields) || Object.keys(fields).length === 0) {
      throw new Error(`notes[${index}].fields must be a non-empty object`);
    }
    const parsedFields: Record<string, string> = {};
    for (const [name, fieldValue] of Object.entries(fields)) {
      if (typeof fieldValue !== "string") {
        throw new Error(`notes[${index}].fields.${name} must be a string`);
      }
      parsedFields[name] = fieldValue;
    }
    if (!Array.isArray(tags) || tags.some((tag) => typeof tag !== "string" || tag.length === 0)) {
      throw new Error(`notes[${index}].tags must be an array of non-empty strings`);
    }
    return { deckName, modelName, fields: parsedFields, tags: tags as string[] };
  });
  return { notes };
}

function transactionTag(): string {
  return `${TRANSACTION_TAG_PREFIX}${randomUUID().replaceAll("-", "")}`;
}

function unique<T>(values: T[]): T[] {
  return [...new Set(values)];
}

async function currentTransactionTags(client: AnkiConnectClient): Promise<string[]> {
  const tags = await client.invoke<string[]>("getTags");
  return tags.filter((tag) => tag.startsWith(TRANSACTION_TAG_PREFIX)).sort();
}

export async function snapshot(client: AnkiConnectClient, query?: string): Promise<JsonRecord> {
  const [apiVersion, decks, tags, models] = await Promise.all([
    client.invoke<number>("version"),
    client.invoke<string[]>("deckNames"),
    client.invoke<string[]>("getTags"),
    client.invoke<string[]>("modelNames"),
  ]);

  let notes: NoteInfo[] = [];
  let cards: CardInfo[] = [];
  if (query !== undefined) {
    const noteIds = await client.invoke<number[]>("findNotes", { query });
    if (noteIds.length > 0) {
      notes = await client.invoke<NoteInfo[]>("notesInfo", { notes: noteIds });
      const cardIds = unique(notes.flatMap((note) => note.cards));
      if (cardIds.length > 0) {
        cards = await client.invoke<CardInfo[]>("cardsInfo", { cards: cardIds });
      }
    }
  }

  return {
    apiVersion,
    decks: [...decks].sort(),
    tags: [...tags].sort(),
    models: [...models].sort(),
    pendingTransactions: tags.filter((tag) => tag.startsWith(TRANSACTION_TAG_PREFIX)).sort(),
    ...(query === undefined ? {} : { query, notes, cards }),
  };
}

export async function modelFields(client: AnkiConnectClient, modelName: string): Promise<JsonRecord> {
  return {
    modelName,
    fields: await client.invoke<string[]>("modelFieldNames", { modelName }),
  };
}

export async function createDeck(client: AnkiConnectClient, deck: string): Promise<JsonRecord> {
  const decks = await client.invoke<string[]>("deckNames");
  if (decks.includes(deck)) {
    return { deck, created: false };
  }
  const deckId = await client.invoke<number>("createDeck", { deck });
  return { deck, deckId, created: true };
}

async function validateTags(client: AnkiConnectClient, notes: NoteInput[]): Promise<void> {
  const existing = new Set(await client.invoke<string[]>("getTags"));
  for (const [noteIndex, note] of notes.entries()) {
    for (const tag of note.tags) {
      if (tag.startsWith(TRANSACTION_TAG_PREFIX)) {
        throw new Error(`notes[${noteIndex}].tags: reserved transaction tag prefix`);
      }
      if (existing.has(tag)) {
        continue;
      }
      if (!NEW_TAG_PATTERN.test(tag)) {
        throw new Error(
          `notes[${noteIndex}].tags: new tag ${JSON.stringify(tag)} must use lowercase letters, digits, hyphens, and :: only`,
        );
      }
    }
  }
}

async function observeTransaction(client: AnkiConnectClient, tag: string): Promise<NoteInfo[]> {
  const ids = await client.invoke<number[]>("findNotes", { query: `tag:${tag}` });
  return ids.length === 0 ? [] : client.invoke<NoteInfo[]>("notesInfo", { notes: ids });
}

async function cleanupTransaction(client: AnkiConnectClient, tag: string, notes: NoteInfo[]): Promise<void> {
  if (notes.length === 0) {
    return;
  }
  await client.invoke<null>("removeTags", {
    notes: notes.map((note) => note.noteId),
    tags: tag,
  });
}

export async function recover(client: AnkiConnectClient): Promise<JsonRecord> {
  const tags = await currentTransactionTags(client);
  const recovered: JsonRecord[] = [];
  for (const tag of tags) {
    const notes = await observeTransaction(client, tag);
    await cleanupTransaction(client, tag, notes);
    recovered.push({ transactionTag: tag, noteIds: notes.map((note) => note.noteId), notes });
  }
  return { recovered };
}

export async function add(client: AnkiConnectClient, input: AddInput): Promise<JsonRecord> {
  await validateTags(client, input.notes);
  const txTag = transactionTag();
  const notes = input.notes.map((note) => ({ ...note, tags: unique([...note.tags, txTag]) }));
  const preflight = await client.invoke<CanAddResult[]>("canAddNotesWithErrorDetail", { notes });
  const blocked = preflight
    .map((result, index) => ({ ...result, index }))
    .filter((result) => !result.canAdd);
  if (blocked.length > 0) {
    return { status: "blocked", added: [], blocked };
  }

  let returnedIds: Array<number | null> | null = null;
  let addError: string | null = null;
  try {
    returnedIds = await client.invoke<Array<number | null>>("addNotes", { notes });
  } catch (error) {
    addError = error instanceof Error ? error.message : String(error);
  }

  let observed: NoteInfo[];
  try {
    observed = await observeTransaction(client, txTag);
  } catch (error) {
    return {
      status: "pending-verification",
      transactionTag: txTag,
      addError,
      verificationError: error instanceof Error ? error.message : String(error),
      returnedIds,
    };
  }

  const observedIds = observed.map((note) => note.noteId).sort((a, b) => a - b);
  const returnedAddedIds = (returnedIds ?? []).filter((id): id is number => id !== null).sort((a, b) => a - b);
  const returnedFailures = (returnedIds ?? [])
    .map((id, index) => ({ id, index }))
    .filter(({ id }) => id === null)
    .map(({ index }) => index);

  try {
    await cleanupTransaction(client, txTag, observed);
  } catch (error) {
    return {
      status: "verified-with-pending-cleanup",
      transactionTag: txTag,
      addError,
      returnedIds,
      returnedFailures,
      observedNoteIds: observedIds,
      cleanupError: error instanceof Error ? error.message : String(error),
    };
  }

  const idsAgree = returnedIds === null || JSON.stringify(returnedAddedIds) === JSON.stringify(observedIds);
  return {
    status:
      addError !== null || returnedFailures.length > 0 || !idsAgree
        ? "partial-or-recovered"
        : "added",
    addError,
    returnedIds,
    returnedFailures,
    observedNoteIds: observedIds,
    notes: observed,
  };
}

async function readStdinJson(): Promise<unknown> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  if (chunks.length === 0) {
    throw new Error("stdin is empty");
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

function usage(): never {
  throw new Error(
    [
      "usage:",
      "  anki-connect.ts snapshot [QUERY]",
      "  anki-connect.ts model-fields MODEL_NAME",
      "  anki-connect.ts create-deck DECK_NAME",
      "  anki-connect.ts add < notes.json",
      "  anki-connect.ts recover",
    ].join("\n"),
  );
}

export async function main(argv = process.argv.slice(2)): Promise<void> {
  const client = new AnkiConnectClient();
  const [command, ...args] = argv;
  let result: JsonRecord;
  switch (command) {
    case "snapshot":
      if (args.length > 1) usage();
      result = await snapshot(client, args[0]);
      break;
    case "model-fields":
      if (args.length !== 1) usage();
      result = await modelFields(client, args[0]);
      break;
    case "create-deck":
      if (args.length !== 1) usage();
      result = await createDeck(client, args[0]);
      break;
    case "add":
      if (args.length !== 0) usage();
      result = await add(client, parseAddInput(await readStdinJson()));
      break;
    case "recover":
      if (args.length !== 0) usage();
      result = await recover(client);
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
