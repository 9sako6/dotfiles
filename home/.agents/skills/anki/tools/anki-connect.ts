#!/usr/bin/env bun

import { randomUUID } from "node:crypto";

const API_VERSION = 6;
const DEFAULT_ENDPOINT = "http://127.0.0.1:8765";
const ALLOWED_HOSTNAMES = ["127.0.0.1", "[::1]", "host.docker.internal", "localhost"];
const TRANSACTION_TAG_PREFIX = "anki-tx-";

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

export type NoteInfo = {
  noteId: number;
  modelName: string;
  tags: string[];
  fields: Record<string, { value: string; order: number }>;
  cards: number[];
};

export type CardInfo = {
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

type AddError = {
  index?: number;
  error: string;
};

export type AddResult =
  | { status: "success"; noteIds: number[] }
  | { status: "rejected"; noteIds: number[]; errors: AddError[] }
  | { status: "partial"; noteIds: number[]; missing: number }
  | { status: "indeterminate"; noteIds: number[]; error: string };

type PendingTransaction = {
  tag: string;
  notes: NoteInfo[];
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
  if (!ALLOWED_HOSTNAMES.includes(url.hostname)) {
    throw new Error("AnkiConnect endpoint must be loopback or host.docker.internal");
  }
  if (url.username !== "" || url.password !== "") {
    throw new Error("AnkiConnect endpoint must not include credentials");
  }
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isCanAddResult(value: unknown): value is CanAddResult {
  return isRecord(value)
    && typeof value.canAdd === "boolean"
    && (value.error === null || typeof value.error === "string");
}

function isAddNotesResult(value: unknown): value is Array<number | null> {
  return Array.isArray(value)
    && value.every((noteId) => noteId === null || typeof noteId === "number");
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

function requireTag(tag: string): string {
  if (tag.length === 0 || /\s/u.test(tag)) {
    throw new Error("tag must be a non-empty Anki tag without whitespace");
  }
  return tag;
}

function escapeRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

export function exactTagQuery(tag: string): string {
  return `tag:re:^${escapeRegex(requireTag(tag))}$`;
}

function transactionTagQuery(): string {
  return `tag:re:^${escapeRegex(TRANSACTION_TAG_PREFIX)}.*$`;
}

export async function notesWithExactTag(client: AnkiConnectClient, tag: string): Promise<NoteInfo[]> {
  const noteIds = await client.invoke<number[]>("findNotes", { query: exactTagQuery(tag) });
  return noteIds.length === 0 ? [] : client.invoke<NoteInfo[]>("notesInfo", { notes: noteIds });
}

function transactionTag(): string {
  return `${TRANSACTION_TAG_PREFIX}${randomUUID().replaceAll("-", "")}`;
}

function unique<T>(values: T[]): T[] {
  return [...new Set(values)];
}

function visibleTags(tags: string[]): string[] {
  return tags.filter((tag) => !tag.startsWith(TRANSACTION_TAG_PREFIX));
}

function visibleNote(note: NoteInfo): NoteInfo {
  return { ...note, tags: visibleTags(note.tags) };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function snapshot(client: AnkiConnectClient, tag?: string): Promise<JsonRecord> {
  const [apiVersion, decks, tags, models] = await Promise.all([
    client.invoke<number>("version"),
    client.invoke<string[]>("deckNames"),
    client.invoke<string[]>("getTags"),
    client.invoke<string[]>("modelNames"),
  ]);

  let notes: NoteInfo[] = [];
  let cards: CardInfo[] = [];
  if (tag !== undefined) {
    const matchedNotes = await notesWithExactTag(client, tag);
    const cardIds = unique(matchedNotes.flatMap((note) => note.cards));
    if (cardIds.length > 0) {
      cards = await client.invoke<CardInfo[]>("cardsInfo", { cards: cardIds });
    }
    notes = matchedNotes.map(visibleNote);
  }

  return {
    apiVersion,
    decks: [...decks].sort(),
    tags: visibleTags(tags).sort(),
    models: [...models].sort(),
    ...(tag === undefined ? {} : { tag, notes, cards }),
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

function assertNoReservedTags(notes: NoteInput[]): void {
  for (const [noteIndex, note] of notes.entries()) {
    for (const tag of note.tags) {
      if (tag.startsWith(TRANSACTION_TAG_PREFIX)) {
        throw new Error(`notes[${noteIndex}].tags: reserved internal tag prefix`);
      }
    }
  }
}

async function pendingTransactions(client: AnkiConnectClient): Promise<PendingTransaction[]> {
  const noteIds = await client.invoke<number[]>("findNotes", { query: transactionTagQuery() });
  if (noteIds.length === 0) {
    return [];
  }

  const notes = await client.invoke<NoteInfo[]>("notesInfo", { notes: noteIds });
  const notesByTag = new Map<string, NoteInfo[]>();
  for (const note of notes) {
    for (const tag of note.tags) {
      if (!tag.startsWith(TRANSACTION_TAG_PREFIX)) {
        continue;
      }
      const taggedNotes = notesByTag.get(tag);
      if (taggedNotes === undefined) {
        notesByTag.set(tag, [note]);
      } else {
        taggedNotes.push(note);
      }
    }
  }

  return [...notesByTag.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([tag, taggedNotes]) => ({ tag, notes: taggedNotes }));
}

async function observeTransaction(client: AnkiConnectClient, tag: string): Promise<NoteInfo[]> {
  return notesWithExactTag(client, tag);
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

async function reconcilePendingTransactions(client: AnkiConnectClient): Promise<boolean> {
  const transactions = await pendingTransactions(client);
  for (const transaction of transactions) {
    await cleanupTransaction(client, transaction.tag, transaction.notes);
  }
  return transactions.length > 0;
}

export async function add(client: AnkiConnectClient, input: AddInput): Promise<AddResult> {
  assertNoReservedTags(input.notes);

  let reconciled: boolean;
  try {
    reconciled = await reconcilePendingTransactions(client);
  } catch (error) {
    return { status: "indeterminate", noteIds: [], error: errorMessage(error) };
  }
  if (reconciled) {
    return {
      status: "rejected",
      noteIds: [],
      errors: [{ error: "A previous Anki write was reconciled. Read Anki again before retrying." }],
    };
  }

  const txTag = transactionTag();
  const notes = input.notes.map((note) => ({ ...note, tags: unique([...note.tags, txTag]) }));

  let rawPreflight: unknown;
  try {
    rawPreflight = await client.invoke<unknown>("canAddNotesWithErrorDetail", { notes });
  } catch (error) {
    return { status: "rejected", noteIds: [], errors: [{ error: errorMessage(error) }] };
  }
  if (!Array.isArray(rawPreflight)
    || rawPreflight.length !== input.notes.length
    || !rawPreflight.every(isCanAddResult)) {
    return {
      status: "rejected",
      noteIds: [],
      errors: [{ error: "Anki returned an invalid preflight result" }],
    };
  }
  const errors = rawPreflight
    .map((result, index) => ({ ...result, index }))
    .filter((result) => !result.canAdd)
    .map(({ index, error }) => ({ index, error: error ?? "Anki rejected the note" }));
  if (errors.length > 0) {
    return { status: "rejected", noteIds: [], errors };
  }

  let returnedIds: Array<number | null> | null = null;
  let writeError: string | null = null;
  try {
    const rawResult = await client.invoke<unknown>("addNotes", { notes });
    if (isAddNotesResult(rawResult)) {
      returnedIds = rawResult;
    } else {
      writeError = "addNotes returned an invalid result";
    }
  } catch (error) {
    writeError = errorMessage(error);
  }

  let observed: NoteInfo[];
  try {
    observed = await observeTransaction(client, txTag);
  } catch (error) {
    return { status: "indeterminate", noteIds: [], error: errorMessage(error) };
  }

  const noteIds = observed.map((note) => note.noteId).sort((a, b) => a - b);
  try {
    await cleanupTransaction(client, txTag, observed);
  } catch {
    // A later add reconciles the leftover internal tag before considering a new write.
  }

  if (observed.length === input.notes.length) {
    return { status: "success", noteIds };
  }

  if (observed.length > input.notes.length) {
    return {
      status: "indeterminate",
      noteIds,
      error: "Anki returned more notes than this request contained",
    };
  }

  if (observed.length > 0) {
    return {
      status: "partial",
      noteIds,
      missing: input.notes.length - observed.length,
    };
  }

  if (returnedIds !== null
    && returnedIds.length === input.notes.length
    && returnedIds.every((id) => id === null)) {
    return {
      status: "rejected",
      noteIds: [],
      errors: [{ error: writeError ?? "Anki rejected all notes" }],
    };
  }

  return {
    status: "indeterminate",
    noteIds: [],
    error: writeError ?? "Anki write result could not be verified",
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
      "  anki-connect.ts snapshot [TAG]",
      "  anki-connect.ts model-fields MODEL_NAME",
      "  anki-connect.ts create-deck DECK_NAME",
      "  anki-connect.ts add < notes.json",
    ].join("\n"),
  );
}

export async function main(argv = process.argv.slice(2)): Promise<void> {
  const client = new AnkiConnectClient();
  const [command, ...args] = argv;
  let result: unknown;
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
