import { describe, expect, it } from "vitest";
import {
  addNativeField,
  addNativeSection,
  newNativeSectionName,
  removeNativeSection,
  changedFields,
  editNativeField,
  nativeSections,
  removeNativeField,
} from "./native-document";
import type { ConfigurationDraft } from "./contracts";

describe("native document navigation", () => {
  it("locates named and anonymous sections, including repeated list values", () => {
    const sections = nativeSections(
      "# synthetic\nconfig interface 'lan'\n option proto 'static'\n list dns '192.0.2.53'\n list dns '192.0.2.54'\nconfig rule\n option name 'Synthetic'\n",
    );
    expect(sections).toHaveLength(2);
    expect(sections[0]).toMatchObject({
      type: "interface",
      name: "lan",
      line: 2,
      offset: 12,
    });
    expect(sections[0].fields).toHaveLength(3);
    expect(sections[1]).toMatchObject({
      type: "rule",
      name: "匿名 2",
      line: 6,
    });
  });
  it("summarizes only changed fields from whole-document native diffs", () => {
    const draft: ConfigurationDraft = {
      id: "test",
      module: "network",
      generation: 1,
      valid: true,
      risks: [],
      errors: [],
      createdAt: "2026-01-01T00:00:00Z",
      diff: "--- a/network\n+++ b/network\n@@ -1,3 +1,3 @@\n-config interface 'lan'\n- option proto 'static'\n- option ipaddr '192.0.2.1'\n+config interface 'lan'\n+ option proto 'static'\n+ option ipaddr '192.0.2.2'",
    };
    expect(changedFields([draft])).toEqual(["network / option ipaddr"]);
  });
});

describe("source-preserving UCI field edits", () => {
  const content = String.raw`# untouched header
config 'interface' "lan" # section comment
  option proto 'static'
  option ipaddr "192.0.2.1" # management address
  option vendor_note 'Alice '\''router # 1' # keep note
  option escaped "a\\b\"c"
  list dns '192.0.2.53' # first resolver
  option mtu 1500
  list dns "192.0.2.54" # second resolver

# keep this line
config rule
  option name 'Untouched'
`;
  it("decodes escaped, adjacent, empty, unquoted and quoted tokens without comments", () => {
    const fields = nativeSections(content)[0].fields;
    expect(fields.map(({ name, value }) => [name, value])).toEqual([
      ["proto", "static"],
      ["ipaddr", "192.0.2.1"],
      ["vendor_note", "Alice 'router # 1"],
      ["escaped", 'a\\b"c'],
      ["dns", "192.0.2.53"],
      ["mtu", "1500"],
      ["dns", "192.0.2.54"],
    ]);
    expect(
      nativeSections(
        "config interface lan\n option empty ''\n option word a\\ b\\#c\n",
      )[0].fields.map((field) => field.value),
    ).toEqual(["", "a b#c"]);
  });
  it("patches only the changed value, preserving quote style, comments, order and unrelated sections", () => {
    const section = nativeSections(content)[0];
    expect(editNativeField(content, section.fields[1], "192.0.2.1")).toBe(
      content,
    );
    const updated = editNativeField(content, section.fields[1], "192.0.2.9");
    expect(updated).toBe(content.replace('"192.0.2.1"', '"192.0.2.9"'));
    const escaped = editNativeField(
      updated,
      nativeSections(updated)[0].fields[2],
      "Bob's \\router # 2",
    );
    expect(nativeSections(escaped)[0].fields[2].value).toBe(
      "Bob's \\router # 2",
    );
    expect(escaped).toContain("# keep note");
    expect(escaped.slice(escaped.indexOf("\n# keep this line"))).toBe(
      content.slice(content.indexOf("\n# keep this line")),
    );
    const double = editNativeField(
      escaped,
      nativeSections(escaped)[0].fields[3],
      'path\\"quoted"',
    );
    expect(nativeSections(double)[0].fields[3].value).toBe('path\\"quoted"');
  });
  it("quotes shell-like punctuation when editing a previously unquoted vendor value", () => {
    const source = "config interface lan\n option vendor_token safe # keep\n";
    for (const value of [
      "literal;value",
      "literal`value",
      "Alice's \\value # note",
    ]) {
      const updated = editNativeField(
        source,
        nativeSections(source)[0].fields[0],
        value,
      );
      expect(updated).toContain("option vendor_token '");
      expect(nativeSections(updated)[0].fields[0].value).toBe(value);
      expect(updated).toContain(" # keep\n");
    }
  });
  it("adds, removes and edits list entries without reserializing intervening options or dropping comments", () => {
    const first = nativeSections(content)[0].fields.find(
      (field) => field.name === "dns",
    )!;
    const removed = removeNativeField(content, first);
    expect(removed).toBe(
      content.replace(
        "  list dns '192.0.2.53' # first resolver",
        "  # first resolver",
      ),
    );
    const added = addNativeField(
      removed,
      nativeSections(removed)[0],
      "dns",
      "list",
      "198.51.100.53",
    );
    expect(added).toContain(
      '  list dns "192.0.2.54" # second resolver\n  list dns "198.51.100.53"\n',
    );
    expect(
      nativeSections(added)[0]
        .fields.filter((field) => field.name === "dns")
        .map((field) => field.value),
    ).toEqual(["192.0.2.54", "198.51.100.53"]);
    expect(added).toContain("  option mtu 1500\n");
    expect(nativeSections(added)[1].fields[0].value).toBe("Untouched");
  });
  it("retains CRLF and handles multiline quoted values and backslash continuations", () => {
    const continued =
      "config interface 'lan'\r\n  option label \\\r\n    \"First\r\nSecond\" # note\r\n  option other 'same'\r\n";
    const sections = nativeSections(continued);
    expect(sections[0].fields[0]).toMatchObject({
      value: "First\r\nSecond",
      line: 2,
    });
    expect(sections[0].fields[1].line).toBe(5);
    const edited = editNativeField(
      continued,
      sections[0].fields[0],
      'Next "label"',
    );
    expect(edited).toBe(
      "config interface 'lan'\r\n  option label \\\r\n    \"Next \\\"label\\\"\" # note\r\n  option other 'same'\r\n",
    );
    const added = addNativeField(
      edited,
      nativeSections(edited)[0],
      "dns",
      "list",
      "192.0.2.53",
    );
    expect(added).toBe(edited + "  list dns '192.0.2.53'\r\n");
  });
  it("never changes malformed or unsupported lines and separates malformed sections", () => {
    const document =
      "config interface lan\n option ipaddr '192.0.2.1'\n vendor_magic a b\n option invalid a b\nconfig\n option vendor 'keep'\n";
    const sections = nativeSections(document);
    expect(sections).toHaveLength(2);
    expect(sections[0].fields).toHaveLength(1);
    const edited = editNativeField(
      document,
      sections[0].fields[0],
      "192.0.2.2",
    );
    expect(edited).toBe(document.replace("192.0.2.1", "192.0.2.2"));
    expect(
      addNativeField(document, sections[0], "invalid field", "option"),
    ).toBe(document);
  });
});

describe("routine section operations", () => {
  it("generates unique IDs and appends sections without rewriting the existing document", () => {
    const content =
      "# existing\r\nconfig host 'panel_host_1'\r\n option mac '02:00:00:00:00:01' # known\r\n option vendor_note \"keep quotes\"\r\n";
    expect(newNativeSectionName(content, "host")).toBe("panel_host_2");
    const added = addNativeSection(content, "host", "panel_host_2", [
      { name: "name", value: "Workstation" },
      { name: "mac", value: "02:00:00:00:00:02" },
      { name: "ip", value: "192.0.2.25" },
    ]);
    expect(added).toBe(
      content +
        "\r\nconfig host 'panel_host_2'\r\n\toption name 'Workstation'\r\n\toption mac '02:00:00:00:00:02'\r\n\toption ip '192.0.2.25'\r\n",
    );
    expect(nativeSections(added)[1].fields.map((field) => field.value)).toEqual(
      ["Workstation", "02:00:00:00:00:02", "192.0.2.25"],
    );
    expect(addNativeSection(content, "host", "panel_host_1", [])).toBe(content);
    expect(addNativeSection(content, "host", "unsafe name", [])).toBe(content);
  });
  it("deletes only the selected section statements, keeping comments and neighboring vendor settings", () => {
    const content =
      "# header\nconfig host 'first' # selected host\n option mac '02:00:00:00:00:01' # keep note\n option vendor_x 'remove with selected section'\n\n# next lease comment\nconfig host 'second'\n option vendor_y \"unchanged\" # quote style\n";
    const removed = removeNativeSection(content, nativeSections(content)[0]);
    expect(removed).toBe(
      "# header\n# selected host\n # keep note\n\n# next lease comment\nconfig host 'second'\n option vendor_y \"unchanged\" # quote style\n",
    );
    expect(nativeSections(removed)).toHaveLength(1);
    expect(nativeSections(removed)[0].name).toBe("second");
  });
});
