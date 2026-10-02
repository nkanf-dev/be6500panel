import type { ConfigurationDraft } from "./contracts";

export interface NativeField {
  kind: "option" | "list";
  name: string;
  value: string;
  line: number;
  /** Source spans let forms patch one token without serializing the document. */
  valueStart: number;
  valueEnd: number;
  statementStart: number;
  statementEnd: number;
  fullEnd: number;
  commentStart?: number;
}
export interface NativeSection {
  id: string;
  type: string;
  name: string;
  line: number;
  offset: number;
  end: number;
  fields: NativeField[];
}
interface Token {
  value: string;
  start: number;
  end: number;
}
interface Statement {
  tokens: Token[];
  start: number;
  end: number;
  fullEnd: number;
  line: number;
  commentStart?: number;
  valid: boolean;
}

/** UCI quoting is data, not shell evaluation. Adjacent quoted/unquoted pieces
 * form one token. Single quotes are literal; backslashes escape outside them.
 * Keep source spans, comments, CRLF, and continued/quoted lines intact.
 * The server's isolated UCI parser still owns validation at Stage.
 */
function statements(content: string): Statement[] {
  const result: Statement[] = [];
  let start = 0;
  let line = 1;
  let statementLine = 1;
  let tokens: Token[] = [];
  let tokenStart: number | undefined;
  let value = "";
  let quote = "";
  let commentStart: number | undefined;
  let valid = true;
  const finishToken = (end: number) => {
    if (tokenStart === undefined) return;
    tokens.push({ value, start: tokenStart, end });
    tokenStart = undefined;
    value = "";
  };
  const finishStatement = (end: number, fullEnd: number) => {
    finishToken(end);
    result.push({
      tokens,
      start,
      end,
      fullEnd,
      line: statementLine,
      commentStart,
      valid: valid && !quote,
    });
    start = fullEnd;
    statementLine = line + 1;
    tokens = [];
    commentStart = undefined;
    valid = true;
  };
  for (let index = 0; index < content.length; index++) {
    const char = content[index];
    if (commentStart !== undefined) {
      if (char === "\n") {
        finishStatement(
          content[index - 1] === "\r" ? index - 1 : index,
          index + 1,
        );
        line++;
      }
      continue;
    }
    if (quote && char === quote) {
      quote = "";
      continue;
    }
    if (char === "\\" && quote !== "'") {
      if (index + 1 === content.length) {
        valid = false;
        continue;
      }
      let next = content[++index];
      if (next === "\r" && content[index + 1] === "\n") next = content[++index];
      if (next === "\n") {
        line++;
      } else {
        tokenStart ??= index - 1;
        value += next;
      }
      continue;
    }
    if (quote) {
      value += char;
      if (char === "\n") line++;
      continue;
    }
    if (char === "'" || char === '"') {
      tokenStart ??= index;
      quote = char;
      continue;
    }
    if (char === "#") {
      finishToken(index);
      commentStart = index;
      continue;
    }
    if (char === "\n") {
      finishStatement(
        content[index - 1] === "\r" ? index - 1 : index,
        index + 1,
      );
      line++;
      continue;
    }
    if (/\s/.test(char)) {
      finishToken(index);
      continue;
    }
    tokenStart ??= index;
    value += char;
  }
  if (start < content.length) finishStatement(content.length, content.length);
  return result;
}

export function nativeSections(content: string): NativeSection[] {
  const sections: NativeSection[] = [];
  for (const statement of statements(content)) {
    const { tokens } = statement;
    if (tokens[0]?.value === "config") {
      const previous = sections.at(-1);
      if (previous) previous.end = statement.start;
      // A malformed section must not attach its fields to the preceding one.
      sections.push({
        id: `section-${sections.length}`,
        type: tokens[1]?.value ?? "未知",
        name: tokens[2]?.value ?? `匿名 ${sections.length + 1}`,
        line: statement.line,
        offset: statement.start,
        end: content.length,
        fields: [],
      });
    } else if (
      statement.valid &&
      tokens.length === 3 &&
      (tokens[0].value === "option" || tokens[0].value === "list")
    ) {
      sections.at(-1)?.fields.push({
        kind: tokens[0].value,
        name: tokens[1].value,
        value: tokens[2].value,
        line: statement.line,
        valueStart: tokens[2].start,
        valueEnd: tokens[2].end,
        statementStart: statement.start,
        statementEnd: statement.end,
        fullEnd: statement.fullEnd,
        commentStart: statement.commentStart,
      });
    }
  }
  return sections;
}

/** Prefer the original quote style. Never interpolate or execute UCI values. */
function quoteValue(value: string, original = "''"): string {
  if (original.startsWith('"') && original.endsWith('"'))
    return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
  if (!/[\s#;'"`\\]/.test(value) && value && !/^["']/.test(original))
    return value;
  return `'${value.replace(/'/g, "'\\''")}'`;
}

export function editNativeField(
  content: string,
  field: NativeField,
  value: string,
): string {
  if (field.value === value) return content;
  return (
    content.slice(0, field.valueStart) +
    quoteValue(value, content.slice(field.valueStart, field.valueEnd)) +
    content.slice(field.valueEnd)
  );
}

/** Remove only this list item; retain its inline comment as a standalone line. */
export function removeNativeField(content: string, field: NativeField): string {
  const comment =
    field.commentStart === undefined
      ? ""
      : (content
          .slice(field.statementStart, field.valueStart)
          .match(/^[\t ]*/)?.[0] ?? "") +
        content.slice(field.commentStart, field.fullEnd);
  return (
    content.slice(0, field.statementStart) +
    comment +
    content.slice(field.fullEnd)
  );
}

export function addNativeField(
  content: string,
  section: NativeSection,
  name: string,
  kind: NativeField["kind"],
  value = "",
): string {
  if (!/^[A-Za-z0-9_]+$/.test(name)) return content;
  const peers = section.fields.filter(
    (field) => field.name === name && field.kind === kind,
  );
  const previous = peers.at(-1) ?? section.fields.at(-1);
  const position = peers.at(-1)?.fullEnd ?? section.end;
  const indent = previous
    ? (content
        .slice(previous.statementStart, previous.valueStart)
        .match(/^[\t ]*/)?.[0] ?? "\t")
    : "\t";
  const newline = content.includes("\r\n") ? "\r\n" : "\n";
  const original = peers[0]
    ? content.slice(peers[0].valueStart, peers[0].valueEnd)
    : "''";
  const prefix = position > 0 && content[position - 1] !== "\n" ? newline : "";
  const added = `${prefix}${indent}${kind} ${name} ${quoteValue(value, original)}${newline}`;
  return content.slice(0, position) + added + content.slice(position);
}

export function nativeSectionLabel(section: NativeSection): string {
  for (const name of ["name", "ssid", "hostname", "target", "ip"]) {
    const value = section.fields.find((field) => field.name === name)?.value;
    if (value) return value;
  }
  if (!section.name.startsWith("panel_") && !section.name.startsWith("匿名 "))
    return section.name;
  const device = section.fields.find((field) => field.name === "device")?.value;
  return device || `配置 ${Number(section.id.replace("section-", "")) + 1}`;
}

export interface NewNativeField {
  name: string;
  value: string;
  kind?: NativeField["kind"];
}

/** Friendly forms create new sections with generated, collision-free UCI IDs. */
export function newNativeSectionName(content: string, type: string): string {
  const stem = type.replace(/[^A-Za-z0-9_]/g, "_");
  const names = new Set(nativeSections(content).map((section) => section.name));
  let index = 1;
  while (names.has(`panel_${stem}_${index}`)) index++;
  return `panel_${stem}_${index}`;
}

export function addNativeSection(
  content: string,
  type: string,
  name: string,
  fields: readonly NewNativeField[],
): string {
  if (
    !/^[A-Za-z0-9_-]+$/.test(type) ||
    !/^[A-Za-z0-9_-]+$/.test(name) ||
    nativeSections(content).some((section) => section.name === name) ||
    fields.some((field) => !/^[A-Za-z0-9_-]+$/.test(field.name))
  )
    return content;
  const newline = content.includes("\r\n") ? "\r\n" : "\n";
  const prefix = !content
    ? ""
    : content.endsWith(newline + newline)
      ? ""
      : content.endsWith(newline)
        ? newline
        : newline + newline;
  const body = [
    `config ${type} ${quoteValue(name)}`,
    ...fields.map(
      (field) =>
        `\t${field.kind ?? "option"} ${field.name} ${quoteValue(field.value)}`,
    ),
  ].join(newline);
  return content + prefix + body + newline;
}

/** Remove config/option/list statements only. Retain comments, blank lines,
 * unknown vendor directives and all neighboring sections byte-for-byte.
 */
export function removeNativeSection(
  content: string,
  section: NativeSection,
): string {
  const owned = statements(content).filter(
    (statement) =>
      statement.start >= section.offset &&
      statement.start < section.end &&
      ["config", "option", "list"].includes(statement.tokens[0]?.value),
  );
  let updated = content;
  for (const statement of owned.reverse()) {
    const comment =
      statement.commentStart === undefined
        ? ""
        : (content
            .slice(statement.start, statement.end)
            .match(/^[\t ]*/)?.[0] ?? "") +
          content.slice(statement.commentStart, statement.fullEnd);
    updated =
      updated.slice(0, statement.start) +
      comment +
      updated.slice(statement.fullEnd);
  }
  return updated;
}

/** Summarize changed field names, never values, in the risk acknowledgment.
 * Whole-document diffs can include unchanged lines on both sides. Cancel those.
 */
export function changedFields(drafts: readonly ConfigurationDraft[]): string[] {
  return [
    ...new Set(
      drafts.flatMap((draft) => {
        const removed = new Map<string, number>();
        const added = new Map<string, number>();
        draft.diff.split("\n").forEach((line) => {
          if (!/^[+-](?![+-])/.test(line)) return;
          const side = line[0] === "+" ? added : removed;
          const content = line.slice(1).trim();
          side.set(content, (side.get(content) ?? 0) + 1);
        });
        return [...new Set([...removed.keys(), ...added.keys()])].flatMap(
          (line) => {
            if ((removed.get(line) ?? 0) === (added.get(line) ?? 0)) return [];
            const field = line.match(
              /^(option|list|config)\s+(['"]?)([^\s'"]+)\2/,
            );
            return field ? [`${draft.module} / ${field[1]} ${field[3]}`] : [];
          },
        );
      }),
    ),
  ];
}
