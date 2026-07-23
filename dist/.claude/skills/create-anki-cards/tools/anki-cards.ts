#!/usr/bin/env bun

import { rename, unlink, writeFile } from "node:fs/promises";
import path from "node:path";
import { randomUUID } from "node:crypto";

type Field = {
  name: string;
  role: "question" | "answer" | "reference" | "media" | "id" | "other";
  required: boolean;
};

type Card = {
  id: string;
  fields: Record<string, string>;
  tags: string[];
  sources: string[];
  notes?: string;
};

type TagPolicy =
  | { mode: "open"; requireAtLeastOne: boolean }
  | {
      mode: "restricted";
      allowed: string[];
      requireAtLeastOne: boolean;
    };

type Project = {
  version: 1;
  contract: {
    mode: "create" | "update";
    output: string;
    preview: string;
    deck: string;
    noteType: string;
    html: boolean;
    fields: Field[];
    tagPolicy: TagPolicy;
    identityField?: string;
  };
  cards: Card[];
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseProject(raw: unknown): Project {
  const errors: string[] = [];
  if (!isRecord(raw)) {
    throw new Error("Validation failed:\n- root: オブジェクトが必要です");
  }
  if (typeof raw.version !== "number") {
    errors.push("version: 数値が必要です");
  }
  const contract = raw.contract;
  if (!isRecord(contract)) {
    errors.push("contract: オブジェクトが必要です");
  } else {
    for (const name of ["output", "preview", "deck", "noteType"] as const) {
      if (typeof contract[name] !== "string" || contract[name].length === 0) {
        errors.push(`contract.${name}: 空でない文字列が必要です`);
      }
    }
    if (!["create", "update"].includes(String(contract.mode))) {
      errors.push("contract.mode: createまたはupdateが必要です");
    }
    if (typeof contract.html !== "boolean") {
      errors.push("contract.html: booleanが必要です");
    }
    if (!Array.isArray(contract.fields)) {
      errors.push("contract.fields: 配列が必要です");
    } else {
      for (const [index, field] of contract.fields.entries()) {
        if (!isRecord(field)) {
          errors.push(`contract.fields[${index}]: オブジェクトが必要です`);
          continue;
        }
        if (typeof field.name !== "string" || field.name.length === 0) {
          errors.push(
            `contract.fields[${index}].name: 空でない文字列が必要です`,
          );
        }
        if (typeof field.role !== "string" || !FIELD_ROLES.has(field.role)) {
          errors.push(
            `contract.fields[${index}].role: 対応していない役割です`,
          );
        }
        if (typeof field.required !== "boolean") {
          errors.push(
            `contract.fields[${index}].required: booleanが必要です`,
          );
        }
      }
    }
    const tagPolicy = contract.tagPolicy;
    if (!isRecord(tagPolicy)) {
      errors.push("contract.tagPolicy: オブジェクトが必要です");
    } else {
      if (!["open", "restricted"].includes(String(tagPolicy.mode))) {
        errors.push("contract.tagPolicy.mode: openまたはrestrictedが必要です");
      }
      if (typeof tagPolicy.requireAtLeastOne !== "boolean") {
        errors.push(
          "contract.tagPolicy.requireAtLeastOne: booleanが必要です",
        );
      }
      if (
        tagPolicy.mode === "restricted" &&
        (!Array.isArray(tagPolicy.allowed) ||
          tagPolicy.allowed.some(
            (tag) => typeof tag !== "string" || tag.length === 0,
          ))
      ) {
        errors.push(
          "contract.tagPolicy.allowed: 空でない文字列の配列が必要です",
        );
      }
    }
    if (
      contract.identityField !== undefined &&
      (typeof contract.identityField !== "string" ||
        contract.identityField.length === 0)
    ) {
      errors.push(
        "contract.identityField: 指定する場合は空でない文字列が必要です",
      );
    }
  }
  if (!Array.isArray(raw.cards)) {
    errors.push("cards: 配列が必要です");
  } else {
    for (const [index, card] of raw.cards.entries()) {
      if (!isRecord(card)) {
        errors.push(`cards[${index}]: オブジェクトが必要です`);
        continue;
      }
      if (typeof card.id !== "string" || card.id.length === 0) {
        errors.push(`cards[${index}].id: 空でない文字列が必要です`);
      }
      if (!isRecord(card.fields)) {
        errors.push(`cards[${index}].fields: オブジェクトが必要です`);
      } else {
        for (const [name, value] of Object.entries(card.fields)) {
          if (typeof value !== "string") {
            errors.push(`cards[${index}].fields.${name}: 文字列が必要です`);
          }
        }
      }
      for (const name of ["tags", "sources"] as const) {
        const values = card[name];
        if (
          !Array.isArray(values) ||
          values.some(
            (value) => typeof value !== "string" || value.length === 0,
          )
        ) {
          errors.push(
            `cards[${index}].${name}: 空でない文字列の配列が必要です`,
          );
        }
      }
      if (card.notes !== undefined && typeof card.notes !== "string") {
        errors.push(`cards[${index}].notes: 文字列が必要です`);
      }
    }
  }
  if (errors.length > 0) {
    throw new Error(`Validation failed:\n- ${errors.join("\n- ")}`);
  }
  return raw as unknown as Project;
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
  const hasGuid = guidsByIdentity !== undefined;
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
      contract.mode === "update" && contract.identityField
        ? card.fields[contract.identityField]
        : undefined;
    lines.push(
      [
        ...(hasGuid ? [guidsByIdentity.get(identity ?? "") ?? ""] : []),
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
  if (project.contract.mode !== "update" || !project.contract.identityField) {
    throw new Error("更新モードにはidentityFieldが必要です");
  }
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
  const identityColumn = columns.indexOf(project.contract.identityField);
  if (identityColumn === -1) {
    throw new Error(
      `Anki書き出しTSVに識別フィールドがありません: ${project.contract.identityField}`,
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
    project.cards.map((card) => card.fields[project.contract.identityField!]),
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
  outputs: ReadonlyArray<{ destination: string; content: string }>,
): Promise<void> {
  const temporaryPaths: string[] = [];
  try {
    for (const output of outputs) {
      const temporaryPath = `${output.destination}.${randomUUID()}.tmp`;
      temporaryPaths.push(temporaryPath);
      await writeFile(temporaryPath, output.content, "utf8");
    }
    for (let index = 0; index < outputs.length; index += 1) {
      await rename(temporaryPaths[index], outputs[index].destination);
    }
  } finally {
    await Promise.all(
      temporaryPaths.map((temporaryPath) =>
        unlink(temporaryPath).catch(() => undefined),
      ),
    );
  }
}

function validateProject(project: Project): QualityWarning[] {
  const seenIds = new Set<string>();
  const errors: string[] = [];
  const warnings: QualityWarning[] = [];
  if (project.version !== 1) {
    errors.push("version: 対応している値は1だけです");
  }
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
  if (project.contract.mode === "update" && !project.contract.identityField) {
    errors.push("contract.identityField: 更新モードでは必須です");
  }
  if (
    project.contract.mode === "update" &&
    project.contract.identityField &&
    !seenFieldNames.has(project.contract.identityField)
  ) {
    errors.push(
      `contract.identityField: 契約にないフィールドです: ${project.contract.identityField}`,
    );
  }
  if (project.contract.mode === "create" && project.contract.identityField) {
    errors.push("contract.identityField: 新規作成モードでは指定できません");
  }
  const contractFields = new Map(
    project.contract.fields.map((field) => [field.name, field]),
  );
  const allowedTags =
    project.contract.tagPolicy.mode === "restricted"
      ? new Set(project.contract.tagPolicy.allowed)
      : undefined;
  const seenIdentities = new Set<string>();
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
    if (
      project.contract.mode === "update" &&
      project.contract.identityField &&
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
  return warnings;
}

async function readProject(
  inputPath: string,
): Promise<{ project: Project; warnings: QualityWarning[] }> {
  const project = parseProject(await Bun.file(inputPath).json());
  return { project, warnings: validateProject(project) };
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
  const { project, warnings } = await readProject(inputPath);
  if (project.contract.mode === "create" && ankiExportPath) {
    throw new Error("新規作成モードでは--anki-exportを指定できません");
  }
  if (project.contract.mode === "update" && !ankiExportPath) {
    throw new Error("更新モードには--anki-exportが必要です");
  }
  const guidsByIdentity = ankiExportPath
    ? readGuidMap(await Bun.file(ankiExportPath).text(), project)
    : undefined;
  const absoluteInputPath = path.resolve(inputPath);
  const directory = path.dirname(absoluteInputPath);
  const outputPath = path.resolve(directory, project.contract.output);
  const previewPath = path.resolve(directory, project.contract.preview);
  if ([outputPath, previewPath].includes(absoluteInputPath)) {
    throw new Error("正規データ自身を上書きできません");
  }
  await replaceOutputs([
    {
      destination: outputPath,
      content: renderTsv(project, guidsByIdentity),
    },
    {
      destination: previewPath,
      content: renderPreview(project),
    },
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
