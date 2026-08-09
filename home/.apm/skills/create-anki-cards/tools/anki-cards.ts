#!/usr/bin/env bun

type Field = {
  name: string;
  role: "question" | "answer" | "reference" | "media" | "id" | "other";
  required: boolean;
};

type TagPolicy =
  | { mode: "open"; requireAtLeastOne: boolean }
  | {
      mode: "restricted";
      allowed: string[];
      requireAtLeastOne: boolean;
    };

type DraftNote = {
  id: string;
  deck?: string;
  noteId?: number;
  fields: Record<string, string>;
  tags: string[];
  sources: string[];
  reason?: string;
};

type Batch = {
  version: 1;
  contract: {
    noteType: string;
    fields: Field[];
    tagPolicy: TagPolicy;
  };
  notes: DraftNote[];
};

type QualityWarning = {
  noteId: string;
  code: "multiple-recall" | "long-answer";
  message: string;
};

type AnkiField = {
  value: string;
  order: number;
};

type AnkiNote = {
  noteId: number;
  profile: string;
  tags: string[];
  fields: Record<string, AnkiField>;
  modelName: string;
  mod: number;
  cards: number[];
};

type AnkiCard = {
  cardId: number;
  note: number;
  deckName: string;
};

type AnkiResponse<T> = {
  result: T;
  error: string | null;
};

const API_VERSION = 5;
const DEFAULT_ANKI_CONNECT_URL = "http://127.0.0.1:8765";
const UNSAFE_CONTROL_CHARACTERS =
  /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/u;
const FIELD_ROLES = new Set<Field["role"]>([
  "question",
  "answer",
  "reference",
  "media",
  "id",
  "other",
]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function rejectUnknownKeys(
  value: Record<string, unknown>,
  location: string,
  allowed: ReadonlySet<string>,
  errors: string[],
): void {
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) {
      errors.push(`${location}.${key}: 未知の項目です`);
    }
  }
}

function parseNonEmptyString(
  value: unknown,
  location: string,
  errors: string[],
): string | undefined {
  if (typeof value !== "string" || value.length === 0) {
    errors.push(`${location}: 空でない文字列が必要です`);
    return undefined;
  }
  return value;
}

function parseBoolean(
  value: unknown,
  location: string,
  errors: string[],
): boolean | undefined {
  if (typeof value !== "boolean") {
    errors.push(`${location}: booleanが必要です`);
    return undefined;
  }
  return value;
}

function parseStringList(
  value: unknown,
  location: string,
  errors: string[],
): string[] | undefined {
  if (!Array.isArray(value)) {
    errors.push(`${location}: 文字列配列が必要です`);
    return undefined;
  }
  const values: string[] = [];
  for (let index = 0; index < value.length; index += 1) {
    const item = parseNonEmptyString(value[index], `${location}[${index}]`, errors);
    if (item !== undefined) {
      values.push(item);
    }
  }
  return values;
}

function parseFields(value: unknown, errors: string[]): Field[] | undefined {
  if (!Array.isArray(value) || value.length === 0) {
    errors.push("contract.fields: 1件以上の配列が必要です");
    return undefined;
  }
  const fields: Field[] = [];
  for (let index = 0; index < value.length; index += 1) {
    const location = `contract.fields[${index}]`;
    const rawField = value[index];
    if (!isRecord(rawField)) {
      errors.push(`${location}: オブジェクトが必要です`);
      continue;
    }
    rejectUnknownKeys(
      rawField,
      location,
      new Set(["name", "required", "role"]),
      errors,
    );
    const name = parseNonEmptyString(rawField.name, `${location}.name`, errors);
    const required = parseBoolean(rawField.required, `${location}.required`, errors);
    const role = FIELD_ROLES.has(rawField.role as Field["role"])
      ? (rawField.role as Field["role"])
      : undefined;
    if (role === undefined) {
      errors.push(`${location}.role: 対応していないroleです`);
    }
    if (name !== undefined && required !== undefined && role !== undefined) {
      fields.push({ name, required, role });
    }
  }
  return fields;
}

function parseTagPolicy(
  value: unknown,
  errors: string[],
): TagPolicy | undefined {
  if (!isRecord(value)) {
    errors.push("contract.tagPolicy: オブジェクトが必要です");
    return undefined;
  }
  const mode = value.mode;
  if (mode !== "open" && mode !== "restricted") {
    errors.push("contract.tagPolicy.mode: openまたはrestrictedが必要です");
    return undefined;
  }
  rejectUnknownKeys(
    value,
    "contract.tagPolicy",
    new Set(["allowed", "mode", "requireAtLeastOne"]),
    errors,
  );
  const requireAtLeastOne = parseBoolean(
    value.requireAtLeastOne,
    "contract.tagPolicy.requireAtLeastOne",
    errors,
  );
  if (mode === "open") {
    if (value.allowed !== undefined) {
      errors.push("contract.tagPolicy.allowed: openでは指定できません");
    }
    return requireAtLeastOne === undefined
      ? undefined
      : { mode, requireAtLeastOne };
  }
  const allowed = parseStringList(
    value.allowed,
    "contract.tagPolicy.allowed",
    errors,
  );
  return allowed === undefined || requireAtLeastOne === undefined
    ? undefined
    : { allowed, mode, requireAtLeastOne };
}

function parseNotes(value: unknown, errors: string[]): DraftNote[] | undefined {
  if (!Array.isArray(value) || value.length === 0) {
    errors.push("notes: 1件以上の配列が必要です");
    return undefined;
  }
  const notes: DraftNote[] = [];
  for (let index = 0; index < value.length; index += 1) {
    const location = `notes[${index}]`;
    const rawNote = value[index];
    if (!isRecord(rawNote)) {
      errors.push(`${location}: オブジェクトが必要です`);
      continue;
    }
    rejectUnknownKeys(
      rawNote,
      location,
      new Set(["deck", "fields", "id", "noteId", "reason", "sources", "tags"]),
      errors,
    );
    const id = parseNonEmptyString(rawNote.id, `${location}.id`, errors);
    let deck: string | undefined;
    if (rawNote.deck !== undefined) {
      deck = parseNonEmptyString(rawNote.deck, `${location}.deck`, errors);
    }
    let noteId: number | undefined;
    if (rawNote.noteId !== undefined) {
      if (!Number.isSafeInteger(rawNote.noteId) || Number(rawNote.noteId) <= 0) {
        errors.push(`${location}.noteId: 正の安全な整数が必要です`);
      } else {
        noteId = Number(rawNote.noteId);
      }
    }
    if (noteId === undefined && deck === undefined) {
      errors.push(`${location}.deck: 新規ノートにはデッキが必要です`);
    }
    if (noteId !== undefined && deck !== undefined) {
      errors.push(`${location}.deck: 更新ノートには指定できません`);
    }
    const fields: Record<string, string> = {};
    if (!isRecord(rawNote.fields)) {
      errors.push(`${location}.fields: オブジェクトが必要です`);
    } else {
      for (const [name, fieldValue] of Object.entries(rawNote.fields)) {
        if (typeof fieldValue !== "string") {
          errors.push(`${location}.fields.${name}: 文字列が必要です`);
        } else {
          fields[name] = fieldValue;
        }
      }
    }
    const tags = parseStringList(rawNote.tags, `${location}.tags`, errors);
    const sources = parseStringList(rawNote.sources, `${location}.sources`, errors);
    let reason: string | undefined;
    if (rawNote.reason !== undefined) {
      reason = parseNonEmptyString(rawNote.reason, `${location}.reason`, errors);
    }
    if (
      id !== undefined &&
      isRecord(rawNote.fields) &&
      tags !== undefined &&
      sources !== undefined
    ) {
      notes.push({
        ...(deck === undefined ? {} : { deck }),
        fields,
        id,
        ...(noteId === undefined ? {} : { noteId }),
        ...(reason === undefined ? {} : { reason }),
        sources,
        tags,
      });
    }
  }
  return notes;
}

function parseBatch(value: unknown): Batch {
  const errors: string[] = [];
  if (!isRecord(value)) {
    throw new Error("Validation failed:\n- root: オブジェクトが必要です");
  }
  rejectUnknownKeys(value, "root", new Set(["contract", "notes", "version"]), errors);
  if (value.version !== 1) {
    errors.push("version: 対応している値は1だけです");
  }
  const rawContract = value.contract;
  let contract: Batch["contract"] | undefined;
  if (!isRecord(rawContract)) {
    errors.push("contract: オブジェクトが必要です");
  } else {
    rejectUnknownKeys(
      rawContract,
      "contract",
      new Set(["fields", "noteType", "tagPolicy"]),
      errors,
    );
    const noteType = parseNonEmptyString(
      rawContract.noteType,
      "contract.noteType",
      errors,
    );
    const fields = parseFields(rawContract.fields, errors);
    const tagPolicy = parseTagPolicy(rawContract.tagPolicy, errors);
    if (
      noteType !== undefined &&
      fields !== undefined &&
      tagPolicy !== undefined
    ) {
      contract = { fields, noteType, tagPolicy };
    }
  }
  const notes = parseNotes(value.notes, errors);
  if (errors.length > 0) {
    throw new Error(`Validation failed:\n- ${errors.join("\n- ")}`);
  }
  if (contract === undefined || notes === undefined) {
    throw new Error("Validation failed:\n- root: batchを確定できません");
  }
  return { contract, notes, version: 1 };
}

function isPortableSourceReference(source: string): boolean {
  if (/^https?:\/\//u.test(source)) {
    try {
      return ["http:", "https:"].includes(new URL(source).protocol);
    } catch {
      return false;
    }
  }
  if (
    source.startsWith("/") ||
    source.startsWith("~") ||
    /^[a-z][a-z0-9+.-]*:/iu.test(source)
  ) {
    return false;
  }
  const parts = source.split(/[\\/]/u);
  return (
    parts.length > 0 &&
    parts.every((part) => part.length > 0 && part !== ".." && part !== ".")
  );
}

function validateBatch(batch: Batch): QualityWarning[] {
  const errors: string[] = [];
  const warnings: QualityWarning[] = [];
  const fieldNames = batch.contract.fields.map((field) => field.name);
  const uniqueFieldNames = new Set(fieldNames);
  if (uniqueFieldNames.size !== fieldNames.length) {
    errors.push("contract.fields: フィールド名が重複しています");
  }
  if (!batch.contract.fields.some((field) => field.role === "question")) {
    errors.push("contract.fields: question roleが必要です");
  }
  if (!batch.contract.fields.some((field) => field.role === "answer")) {
    errors.push("contract.fields: answer roleが必要です");
  }
  for (const [index, field] of batch.contract.fields.entries()) {
    if (/[\t\r\n]/u.test(field.name)) {
      errors.push(`contract.fields[${index}].name: タブまたは改行を含められません`);
    }
    if (UNSAFE_CONTROL_CHARACTERS.test(field.name)) {
      errors.push(`contract.fields[${index}].name: 制御文字を含められません`);
    }
  }
  for (const [name, value] of [["noteType", batch.contract.noteType]] as const) {
    if (/[\t\r\n]/u.test(value) || UNSAFE_CONTROL_CHARACTERS.test(value)) {
      errors.push(`contract.${name}: タブ、改行または制御文字を含められません`);
    }
  }
  const allowedTags =
    batch.contract.tagPolicy.mode === "restricted"
      ? new Set(batch.contract.tagPolicy.allowed)
      : undefined;
  if (
    batch.contract.tagPolicy.mode === "restricted" &&
    allowedTags?.size !== batch.contract.tagPolicy.allowed.length
  ) {
    errors.push("contract.tagPolicy.allowed: タグが重複しています");
  }
  const seenIds = new Set<string>();
  const seenNoteIds = new Set<number>();
  for (let index = 0; index < batch.notes.length; index += 1) {
    const note = batch.notes[index];
    const location = `notes[${index}]`;
    if (
      note.deck !== undefined &&
      (/[\t\r\n]/u.test(note.deck) || UNSAFE_CONTROL_CHARACTERS.test(note.deck))
    ) {
      errors.push(`${location}.deck: タブ、改行または制御文字を含められません`);
    }
    if (/\s/u.test(note.id) || UNSAFE_CONTROL_CHARACTERS.test(note.id)) {
      errors.push(`${location}.id: 空白または制御文字を含められません`);
    }
    if (seenIds.has(note.id)) {
      errors.push(`${location}.id: レビューIDが重複しています: ${note.id}`);
    }
    seenIds.add(note.id);
    if (note.noteId !== undefined) {
      if (seenNoteIds.has(note.noteId)) {
        errors.push(`${location}.noteId: 更新対象が重複しています: ${note.noteId}`);
      }
      seenNoteIds.add(note.noteId);
    }
    for (const field of batch.contract.fields) {
      const value = note.fields[field.name];
      if (value === undefined) {
        errors.push(`${location}.fields.${field.name}: フィールドがありません`);
      } else {
        if (field.required && value.length === 0) {
          errors.push(`${location}.fields.${field.name}: 必須フィールドが空です`);
        }
        if (UNSAFE_CONTROL_CHARACTERS.test(value)) {
          errors.push(`${location}.fields.${field.name}: 制御文字を含められません`);
        }
      }
    }
    for (const name of Object.keys(note.fields)) {
      if (!uniqueFieldNames.has(name)) {
        errors.push(`${location}.fields.${name}: 契約にないフィールドです`);
      }
    }
    const seenTags = new Set<string>();
    for (const [tagIndex, tag] of note.tags.entries()) {
      if (/\s/u.test(tag) || UNSAFE_CONTROL_CHARACTERS.test(tag)) {
        errors.push(`${location}.tags[${tagIndex}]: 空白または制御文字を含められません`);
      }
      if (seenTags.has(tag)) {
        errors.push(`${location}.tags: タグが重複しています: ${tag}`);
      }
      seenTags.add(tag);
      if (allowedTags && !allowedTags.has(tag)) {
        errors.push(`${location}.tags: 許可されていないタグです: ${tag}`);
      }
    }
    if (batch.contract.tagPolicy.requireAtLeastOne && note.tags.length === 0) {
      errors.push(`${location}.tags: タグが必要です`);
    }
    if (note.sources.length === 0) {
      errors.push(`${location}.sources: 一次資料がありません`);
    }
    for (const [sourceIndex, source] of note.sources.entries()) {
      if (
        UNSAFE_CONTROL_CHARACTERS.test(source) ||
        /[\r\n]/u.test(source) ||
        !isPortableSourceReference(source)
      ) {
        errors.push(
          `${location}.sources[${sourceIndex}]: 公開URLまたはリポジトリ相対参照が必要です`,
        );
      }
    }
    const questionText = batch.contract.fields
      .filter((field) => field.role === "question")
      .map((field) => note.fields[field.name] ?? "")
      .join("\n");
    const questionMarkCount = [...questionText].filter((character) =>
      ["?", "？"].includes(character),
    ).length;
    if (
      questionMarkCount > 1 ||
      /(それぞれ|いくつ|何と何|すべて|全て|の順で)/u.test(questionText)
    ) {
      warnings.push({
        code: "multiple-recall",
        message: "複数回答または二重質問の可能性があります",
        noteId: note.id,
      });
    }
    const answerLength = batch.contract.fields
      .filter((field) => field.role === "answer")
      .map((field) => note.fields[field.name] ?? "")
      .join("")
      .replace(/<[^>]*>/gu, "")
      .trim().length;
    if (answerLength > 80) {
      warnings.push({
        code: "long-answer",
        message: `答えが長すぎる可能性があります（${answerLength}文字）`,
        noteId: note.id,
      });
    }
  }
  if (errors.length > 0) {
    throw new Error(`Validation failed:\n- ${errors.join("\n- ")}`);
  }
  return warnings;
}

function parseAnkiConnectUrl(): URL {
  const value = process.env.ANKI_CONNECT_URL ?? DEFAULT_ANKI_CONNECT_URL;
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error("ANKI_CONNECT_URLがURLではありません");
  }
  if (
    url.protocol !== "http:" ||
    !["127.0.0.1", "localhost", "[::1]"].includes(url.hostname) ||
    url.username !== "" ||
    url.password !== "" ||
    (url.pathname !== "/" && url.pathname !== "") ||
    url.search ||
    url.hash
  ) {
    throw new Error("ANKI_CONNECT_URLにはloopbackのHTTP originだけを指定できます");
  }
  return url;
}

class AnkiConnectClient {
  readonly url: URL;
  readonly apiKey: string | undefined;

  constructor() {
    this.url = parseAnkiConnectUrl();
    this.apiKey = process.env.ANKI_CONNECT_API_KEY;
  }

  async invoke<T>(
    action: string,
    params: Record<string, unknown> = {},
  ): Promise<T> {
    let response: Response;
    try {
      response = await fetch(this.url, {
        body: JSON.stringify({
          action,
          version: API_VERSION,
          params,
          ...(this.apiKey === undefined ? {} : { key: this.apiKey }),
        }),
        headers: { "content-type": "application/json" },
        method: "POST",
        signal: AbortSignal.timeout(60_000),
      });
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      throw new Error(`AnkiConnectへ接続できません: ${detail}`);
    }
    if (!response.ok) {
      throw new Error(`AnkiConnectがHTTP ${response.status}を返しました`);
    }
    let payload: unknown;
    try {
      payload = await response.json();
    } catch {
      throw new Error("AnkiConnectの応答がJSONではありません");
    }
    if (!isRecord(payload) || !("result" in payload) || !("error" in payload)) {
      throw new Error("AnkiConnectの応答形式が不正です");
    }
    const error = payload.error;
    if (error !== null) {
      throw new Error(`AnkiConnect ${action}: ${String(error)}`);
    }
    return payload.result as T;
  }
}

async function requireApiVersion(client: AnkiConnectClient): Promise<number> {
  const version = await client.invoke<number>("version");
  if (!Number.isInteger(version) || version < API_VERSION) {
    throw new Error(
      `AnkiConnect API v${API_VERSION}以上が必要です: ${String(version)}`,
    );
  }
  return version;
}

async function readBatchFromStdin(): Promise<{
  batch: Batch;
  warnings: QualityWarning[];
}> {
  const text = await Bun.stdin.text();
  if (text.trim().length === 0) {
    throw new Error("標準入力にJSONが必要です");
  }
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`標準入力がJSONではありません: ${detail}`);
  }
  const batch = parseBatch(raw);
  return { batch, warnings: validateBatch(batch) };
}

function printJson(value: unknown): void {
  process.stdout.write(`${JSON.stringify(value, null, 2)}\n`);
}

async function context(options: string[]): Promise<number> {
  let query: string | undefined;
  if (options.length > 0) {
    if (options.length !== 2 || options[0] !== "--query") {
      throw new Error("Usage: anki-cards.ts context [--query <anki-search>]");
    }
    query = options[1];
  }
  const client = new AnkiConnectClient();
  const apiVersion = await requireApiVersion(client);
  const [profile, decks, noteTypeNames, tags] = await Promise.all([
    client.invoke<string>("getActiveProfile"),
    client.invoke<string[]>("deckNames"),
    client.invoke<string[]>("modelNames"),
    client.invoke<string[]>("getTags"),
  ]);
  const noteTypes = await Promise.all(
    noteTypeNames.map(async (name) => ({
      fields: await client.invoke<string[]>("modelFieldNames", {
        modelName: name,
      }),
      name,
    })),
  );
  let notes: AnkiNote[] | undefined;
  if (query !== undefined) {
    const noteIds = await client.invoke<number[]>("findNotes", { query });
    notes = noteIds.length === 0
      ? []
      : await client.invoke<AnkiNote[]>("notesInfo", { notes: noteIds });
  }
  printJson({
    apiVersion,
    decks: [...decks].sort(),
    noteTypes: noteTypes.sort((left, right) =>
      left.name.localeCompare(right.name),
    ),
    profile,
    tags: [...tags].sort(),
    ...(notes === undefined ? {} : { notes }),
  });
  return 0;
}

async function check(options: string[]): Promise<number> {
  if (options.length > 0) {
    throw new Error("Usage: anki-cards.ts check");
  }
  const { batch, warnings } = await readBatchFromStdin();
  printJson({ checked: batch.notes.length, warnings });
  return 0;
}

function sorted(values: readonly string[]): string[] {
  return [...values].sort();
}

function sameStrings(
  left: readonly string[],
  right: readonly string[],
): boolean {
  return JSON.stringify(sorted(left)) === JSON.stringify(sorted(right));
}

function assertLiveContract(batch: Batch, modelFields: readonly string[]): void {
  const expectedFields = batch.contract.fields.map((field) => field.name);
  if (JSON.stringify(modelFields) !== JSON.stringify(expectedFields)) {
    throw new Error(
      `Ankiのフィールド契約が一致しません（Anki: ${modelFields.join(", ")}、入力: ${expectedFields.join(", ")}）`,
    );
  }
}

function mapAnkiNotes(notes: readonly AnkiNote[]): Map<number, AnkiNote> {
  return new Map(
    notes
      .filter((note) => Number.isSafeInteger(note.noteId))
      .map((note) => [note.noteId, note]),
  );
}

function assertUpdateTargets(batch: Batch, current: readonly AnkiNote[]): void {
  const byId = mapAnkiNotes(current);
  for (const note of batch.notes) {
    if (note.noteId === undefined) {
      continue;
    }
    const existing = byId.get(note.noteId);
    if (existing === undefined) {
      throw new Error(`更新対象のノートがありません: ${note.noteId}`);
    }
    if (existing.modelName !== batch.contract.noteType) {
      throw new Error(
        `更新対象${note.noteId}のノートタイプが一致しません: ${existing.modelName}`,
      );
    }
  }
}

function assertAppliedNotes(batch: Batch, notes: readonly AnkiNote[]): void {
  const byId = mapAnkiNotes(notes);
  for (const draft of batch.notes) {
    if (draft.noteId === undefined) {
      continue;
    }
    const actual = byId.get(draft.noteId);
    if (actual === undefined) {
      throw new Error(`書き込み後にノートを取得できません: ${draft.noteId}`);
    }
    if (actual.modelName !== batch.contract.noteType) {
      throw new Error(`書き込み後のノートタイプが一致しません: ${draft.noteId}`);
    }
    for (const [name, expected] of Object.entries(draft.fields)) {
      if (actual.fields[name]?.value !== expected) {
        throw new Error(
          `書き込み後のフィールドが一致しません: ${draft.noteId}/${name}`,
        );
      }
    }
    if (!sameStrings(actual.tags, draft.tags)) {
      throw new Error(`書き込み後のタグが一致しません: ${draft.noteId}`);
    }
  }
}

async function apply(options: string[]): Promise<number> {
  if (options.length > 0) {
    throw new Error("Usage: anki-cards.ts apply");
  }
  const { batch, warnings } = await readBatchFromStdin();
  const warningsByNote = new Set(warnings.map((warning) => warning.noteId));
  const unacknowledged = batch.notes
    .filter((note) => warningsByNote.has(note.id) && note.reason === undefined)
    .map((note) => note.id);
  if (unacknowledged.length > 0) {
    throw new Error(
      `警告を解消するかreasonに例外理由を記録してください: ${unacknowledged.join(", ")}`,
    );
  }
  const client = new AnkiConnectClient();
  await requireApiVersion(client);
  const updates = batch.notes.filter(
    (note): note is DraftNote & { noteId: number } => note.noteId !== undefined,
  );
  const additions = batch.notes.filter((note) => note.noteId === undefined);
  const [decks, modelFields] = await Promise.all([
    additions.length === 0
      ? Promise.resolve([])
      : client.invoke<string[]>("deckNames"),
    client.invoke<string[]>("modelFieldNames", {
      modelName: batch.contract.noteType,
    }),
  ]);
  assertLiveContract(batch, modelFields);

  for (const addition of additions) {
    if (!decks.includes(addition.deck as string)) {
      throw new Error(`Ankiにデッキがありません: ${addition.deck}`);
    }
  }
  if (updates.length > 0) {
    const current = await client.invoke<AnkiNote[]>("notesInfo", {
      notes: updates.map((note) => note.noteId),
    });
    assertUpdateTargets(batch, current);
    const results = await client.invoke<AnkiResponse<null>[]>("multi", {
      actions: updates.map((note) => ({
        action: "updateNote",
        version: API_VERSION,
        ...(client.apiKey === undefined ? {} : { key: client.apiKey }),
        params: {
          note: { fields: note.fields, id: note.noteId, tags: note.tags },
        },
      })),
    });
    if (results.length !== updates.length) {
      throw new Error(
        "AnkiConnectが全更新の結果を返しませんでした。現在状態を再取得してください",
      );
    }
    const failures = results
      .map((result, index) => ({
        error: result.error,
        noteId: updates[index].noteId,
      }))
      .filter((result) => result.error !== null);
    if (failures.length > 0) {
      throw new Error(
        `AnkiConnectの更新が一部失敗しました。現在状態を再取得してください: ${failures
          .map((failure) => `${failure.noteId}: ${failure.error}`)
          .join("; ")}`,
      );
    }
    const updated = await client.invoke<AnkiNote[]>("notesInfo", {
      notes: updates.map((note) => note.noteId),
    });
    assertAppliedNotes({ ...batch, notes: updates }, updated);
  }

  let createdIds: number[] = [];
  if (additions.length > 0) {
    createdIds = await client.invoke<number[]>("addNotes", {
      notes: additions.map((note) => ({
        deckName: note.deck,
        fields: note.fields,
        modelName: batch.contract.noteType,
        tags: note.tags,
      })),
    });
    if (
      createdIds.length !== additions.length ||
      createdIds.some((noteId) => !Number.isSafeInteger(noteId) || noteId <= 0)
    ) {
      throw new Error("AnkiConnectが追加したノートIDを完全に返しませんでした");
    }
  }

  const appliedBatch: Batch = {
    ...batch,
    notes: batch.notes.map((note) => {
      if (note.noteId !== undefined) {
        return note;
      }
      const additionIndex = additions.indexOf(note);
      return { ...note, noteId: createdIds[additionIndex] };
    }),
  };
  const allNoteIds = appliedBatch.notes.map((note) => note.noteId as number);
  const verified = await client.invoke<AnkiNote[]>("notesInfo", {
    notes: allNoteIds,
  });
  assertAppliedNotes(appliedBatch, verified);

  if (createdIds.length > 0) {
    const createdIdSet = new Set(createdIds);
    const createdCardIds = verified
      .filter((note) => createdIdSet.has(note.noteId))
      .flatMap((note) => note.cards);
    const cards = createdCardIds.length === 0
      ? []
      : await client.invoke<AnkiCard[]>("cardsInfo", { cards: createdCardIds });
    for (const noteId of createdIds) {
      const draft = appliedBatch.notes.find((note) => note.noteId === noteId);
      if (
        !cards.some(
          (card) => card.note === noteId && card.deckName === draft?.deck,
        )
      ) {
        throw new Error(`書き込み後のデッキが一致しません: ${noteId}`);
      }
    }
  }

  printJson({
    created: createdIds,
    updated: updates.map((note) => note.noteId),
    warnings,
  });
  return 0;
}

async function main(): Promise<number> {
  const [command, ...options] = Bun.argv.slice(2);
  if (command === "context") {
    return context(options);
  }
  if (command === "check") {
    return check(options);
  }
  if (command === "apply") {
    return apply(options);
  }
  throw new Error("Usage: anki-cards.ts <context|check|apply>");
}

if (import.meta.main) {
  main()
    .then((exitCode) => {
      process.exitCode = exitCode;
    })
    .catch((error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      process.stderr.write(`Error: ${message}\n`);
      process.exitCode = 1;
    });
}
