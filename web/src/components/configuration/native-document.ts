import type { ConfigurationDraft } from "./contracts";

export interface NativeField {
  kind: "option" | "list";
  name: string;
  value: string;
  line: number;
}
export interface NativeSection {
  id: string;
  type: string;
  name: string;
  line: number;
  offset: number;
  fields: NativeField[];
}
const unquote = (value: string) => {
  const trimmed = value.trim();
  return /^(['"])[\s\S]*\1$/.test(trimmed) ? trimmed.slice(1, -1) : trimmed;
};

/** Navigation only. The server's isolated UCI parser owns actual validation. */
export function nativeSections(content: string): NativeSection[] {
  const sections: NativeSection[] = [];
  let offset = 0;
  content.split("\n").forEach((line, index) => {
    const section = line.match(
      /^\s*config\s+(\S+)(?:\s+((?:'[^']*'|"[^"]*"|[^#\s]+)))?/,
    );
    if (section) {
      sections.push({
        id: `section-${index}`,
        type: unquote(section[1]),
        name: section[2] ? unquote(section[2]) : `匿名 ${sections.length + 1}`,
        line: index + 1,
        offset,
        fields: [],
      });
    } else {
      const field = line.match(/^\s*(option|list)\s+(\S+)\s+(.+)$/);
      const current = sections.at(-1);
      if (field && current)
        current.fields.push({
          kind: field[1] as NativeField["kind"],
          name: unquote(field[2]),
          value: unquote(field[3]),
          line: index + 1,
        });
    }
    offset += line.length + 1;
  });
  return sections;
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
