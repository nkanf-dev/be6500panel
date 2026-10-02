import { describe, expect, it } from "vitest";
import { changedFields, nativeSections } from "./native-document";
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
