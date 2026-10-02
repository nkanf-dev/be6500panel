import { describe, expect, it } from "vitest";
import { nativeConfigDiff } from "./config-diff";
describe("native config diff", () => {
  it("retains only changed span and exact content", () =>
    expect(nativeConfigDiff("{\n  old\n}", "{\n  new\n}")).toBe(
      "@@ 第 2 行 · -1 / +1 @@\n-   old\n+   new",
    ));
  it("marks an unchanged candidate without phantom edits", () =>
    expect(nativeConfigDiff("{}", "{}")).toBe("无内容变化"));
  it("shows added lines", () =>
    expect(nativeConfigDiff("one\nend", "one\ntwo\nend")).toContain("+ two"));
});
