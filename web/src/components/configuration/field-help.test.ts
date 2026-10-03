import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { fieldHelp, fieldHelpCatalog, firmwareContext } from "./field-help";
import { fieldInventory, fieldSchema } from "./field-schema";
import manifest from "./field-help/evidence-manifest.json";

const inventory = fieldInventory();
const evidenceSources = manifest as Record<
  string,
  {
    sha256: string;
    bytes: number;
    lines: number;
  }
>;

const keys = () =>
  inventory.map(
    ({ module, section, field }) => `${module}/${section}/${field}`,
  );

describe("complete firmware field evidence", () => {
  it("covers every canonical field once and has no detached catalog entries", () => {
    const catalogKeys = Object.entries(fieldHelpCatalog).flatMap(
      ([module, sections]) =>
        Object.entries(sections).flatMap(([section, fields]) =>
          Object.keys(fields).map((field) => `${module}/${section}/${field}`),
        ),
    );
    expect(inventory).toHaveLength(395);
    expect(new Set(keys()).size).toBe(inventory.length);
    expect(catalogKeys.sort()).toEqual(keys().sort());
  });

  it.each(inventory)(
    "explains $module/$section/$field with traceable facts",
    ({ module, section, field }) => {
      const help = fieldHelp(module, section, field)!;
      expect(help).toBeDefined();
      expect(fieldSchema(module, section, field).help).toEqual(help);
      expect(help.description.length).toBeGreaterThan(16);
      expect(help.impact.length).toBeGreaterThan(8);
      expect(`${help.description} ${help.impact}`).not.toMatch(
        /填写数字[。；]|待补充|TODO|TBD|通用字段提示/,
      );
      expect(help.evidence.length).toBeGreaterThan(0);
      if (help.discovery) {
        expect(help.discovery).toContain(field);
        expect(help.discovery.length).toBeGreaterThan(25);
      }
      for (const evidence of help.evidence) {
        expect(["Xiaomi RN02 1.0.43", "Xiaomi RN02 1.0.64"]).toContain(
          evidence.firmware,
        );
        expect(evidence.source).not.toMatch(/^\/|\.\.|\\|\n/);
        const source = evidenceSources[evidence.source];
        expect(source, `unregistered source: ${evidence.source}`).toBeDefined();
        expect(Number.isInteger(evidence.line)).toBe(true);
        expect(evidence.line).toBeGreaterThan(0);
        expect(evidence.line).toBeLessThanOrEqual(source.lines);
        expect(Number.isInteger(evidence.endLine ?? evidence.line)).toBe(true);
        expect(evidence.endLine ?? evidence.line).toBeGreaterThanOrEqual(
          evidence.line,
        );
        expect(evidence.endLine ?? evidence.line).toBeLessThanOrEqual(
          source.lines,
        );
        expect(evidence.fact.length).toBeGreaterThan(8);
        expect(evidence.source.startsWith("live-inspection/")).toBe(
          evidence.firmware === "Xiaomi RN02 1.0.64",
        );
        if (evidence.artifact) {
          expect(evidence.artifact.source).not.toMatch(/^\/|\.\.|\\|\n/);
          expect(evidenceSources[evidence.artifact.source]?.sha256).toBe(
            evidence.artifact.sha256,
          );
          if (evidence.artifact.offset !== undefined) {
            expect(Number.isInteger(evidence.artifact.offset)).toBe(true);
            expect(evidence.artifact.offset).toBeGreaterThanOrEqual(0);
            expect(evidence.artifact.offset).toBeLessThan(
              evidenceSources[evidence.artifact.source].bytes,
            );
          }
        }
      }
    },
  );

  it("uses a vendor correction only when one was explicitly researched", () => {
    const help = fieldHelp("wireless", "wifi-device", "band")!;
    expect(help.summary).toBeDefined();
    expect(fieldSchema("wireless", "wifi-device", "band").hint).toBe(
      help.summary,
    );
    expect(
      fieldSchema("wireless", "wifi-device", "band").options?.map(
        (item) => item.value,
      ),
    ).toEqual(["2g", "5g", "6g"]);
    expect(fieldSchema("wireless", "wifi-device", "txpower")).toMatchObject({
      widget: "number",
      min: 0,
      max: 40,
    });
  });

  it("distinguishes an explicit zero query port from the omitted-option strategy", () => {
    const help = fieldHelp("dhcp", "dnsmasq", "queryport")!;
    expect(help.description).toContain("单端口");
    expect(help.summary).toContain("留空");
    expect(fieldSchema("dhcp", "dnsmasq", "queryport").hint).toBe(help.summary);
    expect(fieldSchema("dhcp", "dnsmasq", "queryport")).toMatchObject({
      widget: "number",
      min: 0,
      max: 65535,
    });
  });

  it("does not promise that RA policy two bypasses the vendor WAN6 check", () => {
    const help = fieldHelp("dhcp", "dhcp", "ra_default")!;
    expect(help.summary).toContain("WAN6");
    expect(help.summary).toContain("2");
    expect(help.description).toContain("Router Lifetime");
    expect(
      help.evidence.some(
        (item) => item.source === "usr/sbin/wan6_link_check.sh",
      ),
    ).toBe(true);
  });

  it("stores only valid deterministic hashes and source line counts", () => {
    expect(Object.keys(evidenceSources).length).toBeGreaterThan(0);
    for (const record of Object.values(evidenceSources)) {
      expect(record.sha256).toMatch(/^[a-f0-9]{64}$/);
      expect(Number.isInteger(record.lines)).toBe(true);
      expect(record.lines).toBeGreaterThan(0);
      expect(Number.isInteger(record.bytes)).toBe(true);
      expect(record.bytes).toBeGreaterThan(0);
    }
  });

  it("does not reinterpret arbitrary keys or claim the live firmware is verified", () => {
    expect(fieldHelp("network", "vendor", "disabled")).toBeUndefined();
    expect(fieldHelp("system", "constructor", "toString")).toBeUndefined();
    expect(fieldHelp("network", "interface", "__proto__")).toBeUndefined();
    expect(firmwareContext).toContain("1.0.43");
    expect(firmwareContext).toContain("尚未逐项验证 1.0.64");
  });

  it("contains credential instructions rather than embedded passwords", () => {
    for (const item of inventory.filter(
      ({ module, section, field }) =>
        fieldSchema(module, section, field).widget === "password",
    )) {
      const help = fieldHelp(item.module, item.section, item.field)!;
      expect(help.flags).toContain("credential");
      expect(help.defaultValue).toBeUndefined();
      expect(help.description).not.toMatch(/[A-Za-z0-9+/]{48,}={0,2}/);
    }
  });
});

const firmwareRoot = process.env.BE6500_FIRMWARE_ROOT;
const researchRoot = process.env.BE6500_RESEARCH_ROOT;
it.skipIf(!firmwareRoot || !researchRoot)(
  "validates referenced sources against the mounted stock firmware and research tree",
  () => {
    for (const [source, record] of Object.entries(evidenceSources)) {
      const root =
        source.startsWith("static/") || source.startsWith("live-inspection/")
          ? researchRoot!
          : source.startsWith("docs/")
            ? resolve(process.cwd(), "..")
            : firmwareRoot!;
      const content = readFileSync(resolve(root, source));
      expect(createHash("sha256").update(content).digest("hex"), source).toBe(
        record.sha256,
      );
      expect(content.length, source).toBe(record.bytes);
      expect(content.toString("utf8").split("\n").length, source).toBe(
        record.lines,
      );
    }
  },
);
