#!/usr/bin/env bun

import { lstat, realpath, rename, stat, unlink, writeFile } from "node:fs/promises";
import path from "node:path";
import { randomBytes, randomUUID } from "node:crypto";

type Field = {
  name: string;
  role: "question" | "answer" | "reference" | "media" | "id" | "other";
  required: boolean;
};

type Card = {
  id: string;
  guid?: string;
  fields: Record<string, string>;
  tags: string[];
  sources: string[];
  notes?: string;
};

type UpdateCard = Omit<Card, "guid"> & { guid?: never };

type TagPolicy =
  | { mode: "open"; requireAtLeastOne: boolean }
  | {
      mode: "restricted";
      allowed: string[];
      requireAtLeastOne: boolean;
    };

type CommonContract = {
  output: string;
  preview: string;
  deck: string;
  noteType: string;
  html: boolean;
  fields: Field[];
  tagPolicy: TagPolicy;
};

type Project =
  | {
      version: 1;
      contract: CommonContract & {
        mode: "create";
        guidPolicy: "anki" | "generate";
        identityField?: never;
      };
      cards: Card[];
    }
  | {
      version: 1;
      contract: CommonContract & {
        mode: "update";
        guidPolicy?: never;
        identityField: string;
      };
      cards: UpdateCard[];
    };

type QualityWarning = {
  cardId: string;
  code: "multiple-recall" | "long-answer";
  message: string;
};

const UNSAFE_CONTROL_CHARACTERS =
  /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/u;

const FIELD_ROLES = new Set([
  "question",
  "answer",
  "reference",
  "media",
  "id",
  "other",
]);

function isFieldRole(value: unknown): value is Field["role"] {
  return typeof value === "string" && FIELD_ROLES.has(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function rejectUnknownKeys(
  value: Record<string, unknown>,
  pathName: string,
  allowed: ReadonlySet<string>,
  errors: string[],
) {
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) {
      errors.push(`${pathName}.${key}: 未知のキーです`);
    }
  }
}

function parseNonEmptyString(
  value: unknown,
  pathName: string,
  errors: string[],
): string | undefined {
  if (typeof value !== "string" || value.length === 0) {
    errors.push(`${pathName}: 空でない文字列が必要です`);
    return undefined;
  }
  return value;
}

function parseBoolean(
  value: unknown,
  pathName: string,
  errors: string[],
): boolean | undefined {
  if (typeof value !== "boolean") {
    errors.push(`${pathName}: booleanが必要です`);
    return undefined;
  }
  return value;
}

function parseFields(value: unknown, errors: string[]): Field[] | undefined {
  if (!Array.isArray(value)) {
    errors.push("contract.fields: 配列が必要です");
    return undefined;
  }
  const fields: Field[] = [];
  for (const [index, rawField] of value.entries()) {
    const fieldPath = `contract.fields[${index}]`;
    if (!isRecord(rawField)) {
      errors.push(`${fieldPath}: オブジェクトが必要です`);
      continue;
    }
    rejectUnknownKeys(
      rawField,
      fieldPath,
      new Set(["name", "required", "role"]),
      errors,
    );
    const name = parseNonEmptyString(rawField.name, `${fieldPath}.name`, errors);
    const required = parseBoolean(rawField.required, `${fieldPath}.required`, errors);
    const role = rawField.role;
    if (!isFieldRole(role)) {
      errors.push(`${fieldPath}.role: 対応していない役割です`);
      continue;
    }
    if (name !== undefined && required !== undefined) {
      fields.push({ name, required, role });
    }
  }
  return fields;
}

function parseTagPolicy(value: unknown, errors: string[]): TagPolicy | undefined {
  if (!isRecord(value)) {
    errors.push("contract.tagPolicy: オブジェクトが必要です");
    return undefined;
  }
  const mode = value.mode;
  const allowedKeys = mode === "restricted"
    ? new Set(["allowed", "mode", "requireAtLeastOne"])
    : new Set(["mode", "requireAtLeastOne"]);
  rejectUnknownKeys(value, "contract.tagPolicy", allowedKeys, errors);
  const requireAtLeastOne = parseBoolean(
    value.requireAtLeastOne,
    "contract.tagPolicy.requireAtLeastOne",
    errors,
  );
  if (mode === "open") {
    return requireAtLeastOne === undefined
      ? undefined
      : { mode, requireAtLeastOne };
  }
  if (mode !== "restricted") {
    errors.push("contract.tagPolicy.mode: openまたはrestrictedが必要です");
    return undefined;
  }
  const allowed = value.allowed;
  if (
    !Array.isArray(allowed) ||
    allowed.some((tag) => typeof tag !== "string" || tag.length === 0)
  ) {
    errors.push("contract.tagPolicy.allowed: 空でない文字列の配列が必要です");
    return undefined;
  }
  return requireAtLeastOne === undefined
    ? undefined
    : { allowed, mode, requireAtLeastOne };
}

function parseCards(
  value: unknown,
  mode: "create" | "update" | undefined,
  guidPolicy: "anki" | "generate" | undefined,
  errors: string[],
): Card[] | undefined {
  if (!Array.isArray(value)) {
    errors.push("cards: 配列が必要です");
    return undefined;
  }
  const cards: Card[] = [];
  for (const [index, rawCard] of value.entries()) {
    const cardPath = `cards[${index}]`;
    if (!isRecord(rawCard)) {
      errors.push(`${cardPath}: オブジェクトが必要です`);
      continue;
    }
    rejectUnknownKeys(
      rawCard,
      cardPath,
      new Set(["fields", "guid", "id", "notes", "sources", "tags"]),
      errors,
    );
    const id = parseNonEmptyString(rawCard.id, `${cardPath}.id`, errors);
    let guid: string | undefined;
    if (rawCard.guid !== undefined) {
      guid = parseNonEmptyString(rawCard.guid, `${cardPath}.guid`, errors);
      if (mode !== "create" || guidPolicy !== "generate") {
        errors.push(
          `${cardPath}.guid: createモードかつguidPolicyがgenerateの場合だけ指定できます`,
        );
      }
    }
    const fields: Record<string, string> = {};
    if (!isRecord(rawCard.fields)) {
      errors.push(`${cardPath}.fields: オブジェクトが必要です`);
    } else {
      for (const [name, fieldValue] of Object.entries(rawCard.fields)) {
        if (typeof fieldValue !== "string") {
          errors.push(`${cardPath}.fields.${name}: 文字列が必要です`);
        } else {
          fields[name] = fieldValue;
        }
      }
    }
    const lists: Partial<Record<"sources" | "tags", string[]>> = {};
    for (const name of ["sources", "tags"] as const) {
      const rawList = rawCard[name];
      if (
        !Array.isArray(rawList) ||
        rawList.some((item) => typeof item !== "string" || item.length === 0)
      ) {
        errors.push(`${cardPath}.${name}: 空でない文字列の配列が必要です`);
      } else {
        lists[name] = rawList;
      }
    }
    let notes: string | undefined;
    if (rawCard.notes !== undefined) {
      if (typeof rawCard.notes !== "string") {
        errors.push(`${cardPath}.notes: 文字列が必要です`);
      } else {
        notes = rawCard.notes;
      }
    }
    if (
      id !== undefined &&
      isRecord(rawCard.fields) &&
      lists.sources !== undefined &&
      lists.tags !== undefined
    ) {
      cards.push({ fields, guid, id, notes, sources: lists.sources, tags: lists.tags });
    }
  }
  return cards;
}

function parseProject(raw: unknown): Project {
  const errors: string[] = [];
  if (!isRecord(raw)) {
    throw new Error("Validation failed:\n- root: オブジェクトが必要です");
  }
  rejectUnknownKeys(raw, "root", new Set(["cards", "contract", "version"]), errors);
  if (raw.version !== 1) {
    errors.push("version: 対応している値は1だけです");
  }

  const rawContract = raw.contract;
  let contract: Project["contract"] | undefined;
  let mode: "create" | "update" | undefined;
  let guidPolicy: "anki" | "generate" | undefined;
  if (!isRecord(rawContract)) {
    errors.push("contract: オブジェクトが必要です");
  } else {
    mode = rawContract.mode === "create" || rawContract.mode === "update"
      ? rawContract.mode
      : undefined;
    const contractKeys = new Set([
      "deck",
      "fields",
      "html",
      "mode",
      "noteType",
      "output",
      "preview",
      "tagPolicy",
      ...(mode === "create" ? ["guidPolicy"] : []),
      ...(mode === "update" ? ["identityField"] : []),
    ]);
    rejectUnknownKeys(rawContract, "contract", contractKeys, errors);
    if (mode === undefined) {
      errors.push("contract.mode: createまたはupdateが必要です");
    }
    const output = parseNonEmptyString(rawContract.output, "contract.output", errors);
    const preview = parseNonEmptyString(rawContract.preview, "contract.preview", errors);
    const deck = parseNonEmptyString(rawContract.deck, "contract.deck", errors);
    const noteType = parseNonEmptyString(rawContract.noteType, "contract.noteType", errors);
    const html = parseBoolean(rawContract.html, "contract.html", errors);
    const fields = parseFields(rawContract.fields, errors);
    const tagPolicy = parseTagPolicy(rawContract.tagPolicy, errors);
    if (mode === "create") {
      if (
        rawContract.guidPolicy !== undefined &&
        rawContract.guidPolicy !== "anki" &&
        rawContract.guidPolicy !== "generate"
      ) {
        errors.push("contract.guidPolicy: ankiまたはgenerateが必要です");
      } else {
        guidPolicy = rawContract.guidPolicy ?? "anki";
      }
      if (
        output !== undefined &&
        preview !== undefined &&
        deck !== undefined &&
        noteType !== undefined &&
        html !== undefined &&
        fields !== undefined &&
        tagPolicy !== undefined &&
        guidPolicy !== undefined
      ) {
        contract = {
          deck,
          fields,
          guidPolicy,
          html,
          mode,
          noteType,
          output,
          preview,
          tagPolicy,
        };
      }
    } else if (mode === "update") {
      const identityField = parseNonEmptyString(
        rawContract.identityField,
        "contract.identityField",
        errors,
      );
      if (
        output !== undefined &&
        preview !== undefined &&
        deck !== undefined &&
        noteType !== undefined &&
        html !== undefined &&
        fields !== undefined &&
        tagPolicy !== undefined &&
        identityField !== undefined
      ) {
        contract = {
          deck,
          fields,
          html,
          identityField,
          mode,
          noteType,
          output,
          preview,
          tagPolicy,
        };
      }
    }
  }
  const cards = parseCards(raw.cards, mode, guidPolicy, errors);
  if (errors.length > 0) {
    throw new Error(`Validation failed:\n- ${errors.join("\n- ")}`);
  }
  if (contract === undefined || cards === undefined) {
    throw new Error("Validation failed:\n- root: projectを確定できません");
  }
  if (contract.mode === "create") {
    return { cards, contract, version: 1 };
  }
  return {
    cards: cards.map(({ guid: _, ...card }) => card),
    contract,
    version: 1,
  };
}

function encodeTsv(value: string): string {
  if (!/["\t\r\n]/u.test(value)) {
    return value;
  }
  return `"${value.replaceAll('"', '""')}"`;
}

function renderTsv(
  project: Project,
  guidsByIdentity?: ReadonlyMap<string, string>,
): string {
  const { contract, cards } = project;
  const fieldNames = contract.fields.map(({ name }) => name);
  const hasGuid =
    guidsByIdentity !== undefined ||
    (contract.mode === "create" && contract.guidPolicy === "generate");
  const tagColumn = fieldNames.length + (hasGuid ? 2 : 1);
  const lines = [
    "#separator:tab",
    `#html:${contract.html}`,
    `#notetype:${contract.noteType}`,
    `#deck:${contract.deck}`,
    ...(hasGuid ? ["#guid column:1"] : []),
    `#tags column:${tagColumn}`,
    `#columns:${[...(hasGuid ? ["GUID"] : []), ...fieldNames, "Tags"].join("\t")}`,
  ];
  for (const card of cards) {
    const identity =
      contract.mode === "update"
        ? card.fields[contract.identityField]
        : undefined;
    lines.push(
      [
        ...(hasGuid
          ? [
              guidsByIdentity?.get(identity ?? "") ??
                (contract.mode === "create" ? card.guid ?? "" : ""),
            ]
          : []),
        ...fieldNames.map((name) => encodeTsv(card.fields[name] ?? "")),
        encodeTsv(card.tags.join(" ")),
      ].join("\t"),
    );
  }
  return `${lines.join("\n")}\n`;
}

function parseTsvRows(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let inQuotes = false;
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    if (inQuotes) {
      if (character === '"') {
        if (text[index + 1] === '"') {
          field += '"';
          index += 1;
        } else {
          inQuotes = false;
        }
      } else {
        field += character;
      }
      continue;
    }
    if (character === '"' && field.length === 0) {
      inQuotes = true;
    } else if (character === "\t") {
      row.push(field);
      field = "";
    } else if (character === "\n") {
      row.push(field.endsWith("\r") ? field.slice(0, -1) : field);
      rows.push(row);
      row = [];
      field = "";
    } else {
      field += character;
    }
  }
  if (inQuotes) {
    throw new Error("Anki書き出しTSVの引用符が閉じていません");
  }
  if (field.length > 0 || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  return rows;
}

function splitAnkiHeaders(text: string): {
  headers: Map<string, string>;
  body: string;
} {
  const normalized = text.startsWith("\uFEFF") ? text.slice(1) : text;
  const headers = new Map<string, string>();
  let offset = 0;
  while (normalized[offset] === "#") {
    const lineEnd = normalized.indexOf("\n", offset);
    const end = lineEnd === -1 ? normalized.length : lineEnd;
    const line = normalized.slice(offset + 1, end).replace(/\r$/u, "");
    const colon = line.indexOf(":");
    if (colon === -1) {
      throw new Error(`Anki書き出しTSVのヘッダーが不正です: #${line}`);
    }
    const name = line.slice(0, colon);
    if (headers.has(name)) {
      throw new Error(`Anki書き出しTSVのヘッダーが重複しています: #${name}`);
    }
    headers.set(name, line.slice(colon + 1));
    offset = lineEnd === -1 ? normalized.length : lineEnd + 1;
  }
  return { headers, body: normalized.slice(offset) };
}

function readGuidMap(
  text: string,
  project: Project,
): ReadonlyMap<string, string> {
  if (project.contract.mode !== "update") {
    throw new Error("Anki書き出しは更新モードだけで使えます");
  }
  const identityField = project.contract.identityField;
  const { headers, body } = splitAnkiHeaders(text);
  if (headers.get("separator")?.toLowerCase() !== "tab") {
    throw new Error("Anki書き出しTSVには#separator:tabが必要です");
  }
  const fixedNoteType = headers.get("notetype");
  if (fixedNoteType && fixedNoteType !== project.contract.noteType) {
    throw new Error(
      `Anki書き出しTSVのノートタイプが契約と一致しません: ${fixedNoteType}`,
    );
  }
  const columns = headers.get("columns")?.split("\t");
  if (!columns) {
    throw new Error("Anki書き出しTSVには#columnsが必要です");
  }
  if (fixedNoteType && headers.has("notetype column")) {
    throw new Error(
      "Anki書き出しTSVで#notetypeと#notetype columnは併用できません",
    );
  }
  const noteTypeColumn = headers.has("notetype column")
    ? Number(headers.get("notetype column")) - 1
    : undefined;
  if (
    noteTypeColumn !== undefined &&
    (!Number.isInteger(noteTypeColumn) ||
      noteTypeColumn < 0 ||
      noteTypeColumn >= columns.length)
  ) {
    throw new Error("Anki書き出しTSVの#notetype columnが不正です");
  }
  const guidColumn = Number(headers.get("guid column")) - 1;
  if (
    !Number.isInteger(guidColumn) ||
    guidColumn < 0 ||
    guidColumn >= columns.length
  ) {
    throw new Error("Anki書き出しTSVには有効な#guid columnが必要です");
  }
  const identityColumn = columns.indexOf(identityField);
  if (identityColumn === -1) {
    throw new Error(
      `Anki書き出しTSVに識別フィールドがありません: ${identityField}`,
    );
  }
  const guidsByIdentity = new Map<string, string>();
  const seenGuids = new Set<string>();
  for (const [index, row] of parseTsvRows(body).entries()) {
    if (row.length !== columns.length) {
      throw new Error(
        `Anki書き出しTSVの${index + 1}行目は${columns.length}列ではありません`,
      );
    }
    if (
      noteTypeColumn !== undefined &&
      row[noteTypeColumn] !== project.contract.noteType
    ) {
      throw new Error(
        `Anki書き出しTSVの${index + 1}行目のノートタイプが契約と一致しません: ${row[noteTypeColumn]}`,
      );
    }
    const guid = row[guidColumn];
    const identity = row[identityColumn];
    if (!guid || !identity) {
      throw new Error(
        `Anki書き出しTSVの${index + 1}行目にGUIDまたは識別値がありません`,
      );
    }
    if (seenGuids.has(guid)) {
      throw new Error(`Anki書き出しTSVのGUIDが重複しています: ${guid}`);
    }
    if (guidsByIdentity.has(identity)) {
      throw new Error(`Anki書き出しTSVの識別値が重複しています: ${identity}`);
    }
    seenGuids.add(guid);
    guidsByIdentity.set(identity, guid);
  }
  const expectedIdentities = new Set(
    project.cards.map((card) => card.fields[identityField]),
  );
  const missing = [...expectedIdentities].filter(
    (identity) => !guidsByIdentity.has(identity),
  );
  const extra = [...guidsByIdentity.keys()].filter(
    (identity) => !expectedIdentities.has(identity),
  );
  if (missing.length > 0 || extra.length > 0) {
    throw new Error(
      `Anki書き出しTSVとカードを完全照合できません（不足: ${missing.join(", ") || "なし"}、余剰: ${extra.join(", ") || "なし"}）`,
    );
  }
  return guidsByIdentity;
}

function renderPreview(project: Project): string {
  const lines = [
    `# ${project.contract.deck} Ankiカードプレビュー`,
    "",
    `- ノートタイプ: ${project.contract.noteType}`,
    `- カード数: ${project.cards.length}`,
    "",
  ];
  for (const card of project.cards) {
    lines.push(`## ${card.id}`, "");
    for (const field of project.contract.fields) {
      lines.push(`### ${field.name}`, "", card.fields[field.name] || "_空欄_", "");
    }
    lines.push("### タグ", "", card.tags.join(", ") || "_なし_", "");
    lines.push("### 一次資料", "");
    lines.push(
      ...(card.sources.length
        ? card.sources.map((source) => `- ${source}`)
        : ["_なし_"]),
      "",
    );
    if (card.notes) {
      lines.push("### 注記", "", card.notes, "");
    }
  }
  return `${lines.join("\n").trimEnd()}\n`;
}

async function replaceOutputs(
  projectDirectory: string,
  outputs: ReadonlyArray<{
    parent: string;
    filename: string;
    content: string;
  }>,
): Promise<void> {
  const originalDirectory = process.cwd();
  const prepared = outputs.map((output) => ({
    ...output,
    backupName: `.${output.filename}.${randomUUID()}.backup`,
    hadOriginal: false,
    phase: "pending" as "pending" | "staged" | "original-moved" | "published" | "restored",
    temporaryName: `.${output.filename}.${randomUUID()}.tmp`,
  }));
  let published = false;
  try {
    for (const output of prepared) {
      await enterVerifiedOutputParent(projectDirectory, output.parent);

      try {
        const destination = await lstat(output.filename);
        if (destination.isDirectory()) {
          throw new Error(`出力先をdirectoryで置き換えられません: ${output.filename}`);
        }
        output.hadOriginal = true;
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
          throw error;
        }
      }
      await writeFile(output.temporaryName, output.content, {
        encoding: "utf8",
        flag: "wx",
        mode: 0o600,
      });
      output.phase = "staged";
      process.chdir(originalDirectory);
    }

    try {
      for (const output of prepared) {
        await enterVerifiedOutputParent(projectDirectory, output.parent);
        if (output.hadOriginal) {
          await rename(output.filename, output.backupName);
          output.phase = "original-moved";
        }
        await rename(output.temporaryName, output.filename);
        output.phase = "published";
        process.chdir(originalDirectory);
      }
      published = true;
    } catch (error) {
      const rollbackErrors: unknown[] = [];
      for (const output of [...prepared].reverse()) {
        try {
          process.chdir(output.parent);
          if (output.phase === "published") {
            await unlink(output.filename);
          }
          if (output.phase === "published" || output.phase === "original-moved") {
            if (output.hadOriginal) {
              await rename(output.backupName, output.filename);
            }
            output.phase = "restored";
          }
        } catch (rollbackError) {
          rollbackErrors.push(rollbackError);
        } finally {
          process.chdir(originalDirectory);
        }
      }
      if (rollbackErrors.length > 0) {
        throw new AggregateError(
          [error, ...rollbackErrors],
          "output replacement failed and rollback was incomplete",
        );
      }
      throw error;
    }
  } finally {
    process.chdir(originalDirectory);
    for (const output of prepared) {
      await unlink(path.join(output.parent, output.temporaryName)).catch(() => undefined);
      if (published) {
        await unlink(path.join(output.parent, output.backupName)).catch(() => undefined);
      }
    }
  }
}

async function enterVerifiedOutputParent(
  projectDirectory: string,
  outputParent: string,
): Promise<void> {
  process.chdir(outputParent);
  const [currentDirectory, expected, current] = await Promise.all([
    realpath("."),
    stat(outputParent),
    stat("."),
  ]);
  if (
    !isWithinDirectory(projectDirectory, currentDirectory) ||
    expected.dev !== current.dev ||
    expected.ino !== current.ino
  ) {
    throw new Error("出力先の親directoryが検証後に変更されました");
  }
}

function isWithinDirectory(directory: string, candidate: string): boolean {
  const relative = path.relative(directory, candidate);
  return (
    relative === "" ||
    (relative !== ".." &&
      !relative.startsWith(`..${path.sep}`) &&
      !path.isAbsolute(relative))
  );
}

async function resolveOutputTargets(
  projectDirectory: string,
  inputPath: string,
  outputs: ReadonlyArray<{
    name: "output" | "preview";
    destination: string;
    content: string;
  }>,
): Promise<
  Array<{
    name: "output" | "preview";
    parent: string;
    filename: string;
    identity: string;
    content: string;
  }>
> {
  const resolved = await Promise.all(
    outputs.map(async (output) => {
      const parent = await realpath(path.dirname(output.destination));
      if (!isWithinDirectory(projectDirectory, parent)) {
        throw new Error(
          `contract.${output.name}: 出力先の親directoryがproject外です`,
        );
      }
      const filename = path.basename(output.destination);
      let identity: string;
      try {
        identity = await realpath(output.destination);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
          throw error;
        }
        identity = path.join(parent, filename);
      }
      if (!isWithinDirectory(projectDirectory, identity)) {
        throw new Error(`contract.${output.name}: 出力先の実体がproject外です`);
      }
      return { ...output, parent, filename, identity };
    }),
  );

  if (resolved.some(({ identity }) => identity === inputPath)) {
    throw new Error("正規データ自身を上書きできません");
  }
  if (resolved[0].identity === resolved[1].identity) {
    throw new Error("contract.preview: outputとは異なる実体pathを指定してください");
  }
  return resolved;
}

function isPortableSourceReference(source: string): boolean {
  if (/^https?:\/\//u.test(source)) {
    try {
      const url = new URL(source);
      return (
        ["http:", "https:"].includes(url.protocol) &&
        url.hostname.length > 0 &&
        url.hostname !== "localhost" &&
        !url.hostname.endsWith(".local") &&
        url.username === "" &&
        url.password === ""
      );
    } catch {
      return false;
    }
  }
  if (
    source.startsWith("~") ||
    source.startsWith("$") ||
    path.isAbsolute(source) ||
    /^[A-Za-z]:[\\/]/u.test(source) ||
    /^\\\\/u.test(source) ||
    /^(?:file|vscode|vscode-insiders):/iu.test(source) ||
    /^[A-Za-z][A-Za-z0-9+.-]*:\/\//u.test(source) ||
    /[\r\n]/u.test(source) ||
    UNSAFE_CONTROL_CHARACTERS.test(source)
  ) {
    return false;
  }
  const pathPart = source.split("#", 1)[0];
  const normalized = path.normalize(pathPart);
  return (
    pathPart.length > 0 &&
    normalized !== "." &&
    normalized !== ".." &&
    !normalized.startsWith(`..${path.sep}`)
  );
}

function validateProject(project: Project): {
  project: Project;
  warnings: QualityWarning[];
} {
  const seenIds = new Set<string>();
  const errors: string[] = [];
  const warnings: QualityWarning[] = [];
  for (const [name, value] of [
    ["deck", project.contract.deck],
    ["noteType", project.contract.noteType],
  ] as const) {
    if (/[\t\r\n]/u.test(value) || UNSAFE_CONTROL_CHARACTERS.test(value)) {
      errors.push(
        `contract.${name}: 改行または制御文字を含められません`,
      );
    }
  }
  for (const [name, value] of [
    ["output", project.contract.output],
    ["preview", project.contract.preview],
  ] as const) {
    const normalized = path.normalize(value);
    if (
      !value ||
      path.isAbsolute(value) ||
      /[\r\n]/u.test(value) ||
      UNSAFE_CONTROL_CHARACTERS.test(value) ||
      normalized === ".." ||
      normalized.startsWith(`..${path.sep}`)
    ) {
      errors.push(
        `contract.${name}: 作業ディレクトリ内の相対パスを指定してください`,
      );
    }
  }
  if (project.contract.output === project.contract.preview) {
    errors.push("contract.preview: outputとは異なるパスを指定してください");
  }
  const seenFieldNames = new Set<string>();
  for (let index = 0; index < project.contract.fields.length; index += 1) {
    const name = project.contract.fields[index].name;
    if (/[\t\r\n]/u.test(name) || UNSAFE_CONTROL_CHARACTERS.test(name)) {
      errors.push(
        `contract.fields[${index}].name: タブ、改行または制御文字を含められません`,
      );
    }
    if (seenFieldNames.has(name)) {
      errors.push(
        `contract.fields[${index}].name: フィールド名が重複しています: ${name}`,
      );
    }
    seenFieldNames.add(name);
  }
  if (
    project.contract.mode === "update" &&
    !seenFieldNames.has(project.contract.identityField)
  ) {
    errors.push(
      `contract.identityField: 契約にないフィールドです: ${project.contract.identityField}`,
    );
  }
  const contractFields = new Map(
    project.contract.fields.map((field) => [field.name, field]),
  );
  const allowedTags =
    project.contract.tagPolicy.mode === "restricted"
      ? new Set(project.contract.tagPolicy.allowed)
      : undefined;
  const seenIdentities = new Set<string>();
  const seenGuids = new Set<string>();
  for (let index = 0; index < project.cards.length; index += 1) {
    const card = project.cards[index];
    if (/\s/u.test(card.id) || UNSAFE_CONTROL_CHARACTERS.test(card.id)) {
      errors.push(`cards[${index}].id: 空白または制御文字を含められません`);
    }
    if (seenIds.has(card.id)) {
      errors.push(
        `cards[${index}].id: レビューIDが重複しています: ${card.id}`,
      );
    }
    seenIds.add(card.id);
    if (card.guid !== undefined) {
      if (
        project.contract.mode !== "create" ||
        project.contract.guidPolicy !== "generate"
      ) {
        errors.push(
          `cards[${index}].guid: createモードかつguidPolicyがgenerateの場合だけ指定できます`,
        );
      } else if (
        card.guid.length > 10 ||
        [...card.guid].some(
          (character) => !ANKI_BASE91_TABLE.includes(character),
        )
      ) {
        errors.push(
          `cards[${index}].guid: Anki互換のbase91 GUIDではありません`,
        );
      } else if (seenGuids.has(card.guid)) {
        errors.push(`cards[${index}].guid: GUIDが重複しています: ${card.guid}`);
      }
      seenGuids.add(card.guid);
    }
    for (const [name, field] of contractFields) {
      if (field.required && !card.fields[name]) {
        errors.push(`cards[${index}].fields.${name}: 必須フィールドがありません`);
      }
    }
    for (const name of Object.keys(card.fields)) {
      if (!contractFields.has(name)) {
        errors.push(`cards[${index}].fields.${name}: 契約にないフィールドです`);
      }
      if (UNSAFE_CONTROL_CHARACTERS.test(card.fields[name])) {
        errors.push(
          `cards[${index}].fields.${name}: 使用できない制御文字を含んでいます`,
        );
      }
    }
    const seenTags = new Set<string>();
    for (let tagIndex = 0; tagIndex < card.tags.length; tagIndex += 1) {
      const tag = card.tags[tagIndex];
      if (/\s/u.test(tag) || UNSAFE_CONTROL_CHARACTERS.test(tag)) {
        errors.push(
          `cards[${index}].tags[${tagIndex}]: 空白または制御文字を含められません`,
        );
      }
      if (seenTags.has(tag)) {
        errors.push(`cards[${index}].tags: タグが重複しています: ${tag}`);
      }
      seenTags.add(tag);
      if (allowedTags && !allowedTags.has(tag)) {
        errors.push(`cards[${index}].tags: 許可されていないタグです: ${tag}`);
      }
    }
    if (
      project.contract.tagPolicy.requireAtLeastOne &&
      card.tags.length === 0
    ) {
      errors.push(`cards[${index}].tags: タグが必要です`);
    }
    if (card.sources.length === 0) {
      errors.push(`cards[${index}].sources: 一次資料がありません`);
    }
    for (let sourceIndex = 0; sourceIndex < card.sources.length; sourceIndex += 1) {
      if (!isPortableSourceReference(card.sources[sourceIndex])) {
        errors.push(
          `cards[${index}].sources[${sourceIndex}]: 公開URLまたはrepository相対参照が必要です`,
        );
      }
    }
    if (
      project.contract.mode === "update" &&
      contractFields.has(project.contract.identityField)
    ) {
      const identity = card.fields[project.contract.identityField];
      if (!identity) {
        errors.push(
          `cards[${index}].fields.${project.contract.identityField}: 更新識別値が必要です`,
        );
      } else if (seenIdentities.has(identity)) {
        errors.push(
          `cards[${index}].fields.${project.contract.identityField}: 更新識別値が重複しています`,
        );
      }
      seenIdentities.add(identity);
    }
    const questions = project.contract.fields
      .filter((field) => field.role === "question")
      .map((field) => card.fields[field.name] ?? "");
    const questionText = questions.join("\n");
    const questionMarkCount = [...questionText].filter((character) =>
      ["?", "？"].includes(character),
    ).length;
    if (
      questionMarkCount > 1 ||
      /(それぞれ|いくつ|何と何|すべて|全て|の順で)/u.test(questionText)
    ) {
      warnings.push({
        cardId: card.id,
        code: "multiple-recall",
        message: "複数回答または二重質問の可能性があります",
      });
    }
    const answerLength = project.contract.fields
      .filter((field) => field.role === "answer")
      .map((field) => card.fields[field.name] ?? "")
      .join("")
      .replace(/<[^>]*>/gu, "")
      .trim().length;
    if (answerLength > 80) {
      warnings.push({
        cardId: card.id,
        code: "long-answer",
        message: `答えが長すぎる可能性があります（${answerLength}文字）`,
      });
    }
  }
  if (errors.length > 0) {
    throw new Error(`Validation failed:\n- ${errors.join("\n- ")}`);
  }
  return { project, warnings };
}

async function readProject(
  inputPath: string,
): Promise<{ project: Project; warnings: QualityWarning[] }> {
  return validateProject(parseProject(await Bun.file(inputPath).json()));
}

const ANKI_BASE91_TABLE =
  "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!#$%&()*+,-./:;<=>?@[]^_`{|}~";

export function ankiBase91(value: bigint): string {
  let remaining = value;
  let encoded = "";
  const radix = BigInt(ANKI_BASE91_TABLE.length);
  while (remaining > 0n) {
    const remainder = Number(remaining % radix);
    encoded = ANKI_BASE91_TABLE[remainder] + encoded;
    remaining /= radix;
  }
  return encoded;
}

function newAnkiGuid(used: ReadonlySet<string>): string {
  while (true) {
    const candidate = ankiBase91(randomBytes(8).readBigUInt64BE());
    if (candidate && !used.has(candidate)) {
      return candidate;
    }
  }
}

function assignMissingGuids(project: Project): boolean {
  if (
    project.contract.mode !== "create" ||
    project.contract.guidPolicy !== "generate"
  ) {
    return false;
  }
  const used = new Set(
    project.cards
      .map((card) => card.guid)
      .filter((guid): guid is string => guid !== undefined),
  );
  let changed = false;
  for (const card of project.cards) {
    if (card.guid === undefined) {
      card.guid = newAnkiGuid(used);
      used.add(card.guid);
      changed = true;
    }
  }
  return changed;
}

function printWarnings(warnings: QualityWarning[]): void {
  for (const warning of warnings) {
    process.stderr.write(
      `Warning [${warning.cardId}/${warning.code}]: ${warning.message}\n`,
    );
  }
}

async function build(
  inputPath: string,
  ankiExportPath?: string,
): Promise<number> {
  const realInputPath = await realpath(path.resolve(inputPath));
  const { project, warnings } = await readProject(realInputPath);
  if (project.contract.mode === "create" && ankiExportPath) {
    throw new Error("新規作成モードでは--anki-exportを指定できません");
  }
  if (project.contract.mode === "update" && !ankiExportPath) {
    throw new Error("更新モードには--anki-exportが必要です");
  }
  const guidsByIdentity = ankiExportPath
    ? readGuidMap(await Bun.file(ankiExportPath).text(), project)
    : undefined;
  const directory = path.dirname(realInputPath);
  const outputPath = path.resolve(directory, project.contract.output);
  const previewPath = path.resolve(directory, project.contract.preview);
  const assignedGuids = assignMissingGuids(project);
  const resolvedOutputs = await resolveOutputTargets(
    directory,
    realInputPath,
    [
      {
        name: "output",
        destination: outputPath,
        content: renderTsv(project, guidsByIdentity),
      },
      {
        name: "preview",
        destination: previewPath,
        content: renderPreview(project),
      },
    ],
  );
  await replaceOutputs(directory, [
    ...(assignedGuids
      ? [
          {
            parent: directory,
            filename: path.basename(realInputPath),
            content: `${JSON.stringify(project, null, 2)}\n`,
          },
        ]
      : []),
    ...resolvedOutputs,
  ]);
  printWarnings(warnings);
  process.stdout.write(
    `Built ${project.cards.length} ${project.cards.length === 1 ? "card" : "cards"}.\n`,
  );
  return 0;
}

async function check(inputPath: string): Promise<number> {
  const { project, warnings } = await readProject(inputPath);
  printWarnings(warnings);
  process.stdout.write(
    `Checked ${project.cards.length} ${project.cards.length === 1 ? "card" : "cards"}: ${warnings.length} ${warnings.length === 1 ? "warning" : "warnings"}.\n`,
  );
  return 0;
}

async function main(): Promise<number> {
  const [command, inputPath, ...options] = Bun.argv.slice(2);
  if (!inputPath || !["build", "check"].includes(command)) {
    throw new Error(
      "Usage: anki-cards.ts <check|build> <anki.json> [--anki-export <exported.tsv>]",
    );
  }
  if (command === "check") {
    if (options.length > 0) {
      throw new Error("checkには追加オプションを指定できません");
    }
    return check(inputPath);
  }
  let ankiExportPath: string | undefined;
  if (options.length > 0) {
    if (options.length !== 2 || options[0] !== "--anki-export") {
      throw new Error("buildのオプションは--anki-export <file>だけです");
    }
    ankiExportPath = options[1];
  }
  return build(inputPath, ankiExportPath);
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
