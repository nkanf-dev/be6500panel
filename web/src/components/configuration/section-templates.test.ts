import { describe, expect, it } from "vitest";
import type { ConfigurationModule } from "./contracts";
import { addNativeSection, nativeSections } from "./native-document";
import {
  sectionTemplates,
  templateErrors,
  type SectionTemplate,
} from "./section-templates";

function template(module: ConfigurationModule, type: string): SectionTemplate {
  const found = sectionTemplates(module).find((item) => item.type === type);
  if (!found) throw new Error(`Missing ${module} / ${type} template`);
  return found;
}

function valuesFor(
  item: SectionTemplate,
  values: Record<string, string> = {},
): Record<string, string> {
  return {
    ...Object.fromEntries(
      item.fields
        .filter((field) => field.defaultValue !== undefined)
        .map((field) => [field.name, field.defaultValue!]),
    ),
    ...values,
  };
}

const host = () => template("dhcp", "host");
const rule = () => template("firewall", "rule");
const redirect = () => template("firewall", "redirect");
const networkInterface = () => template("network", "interface");
const route = () => template("network", "route");
const wifi = () => template("wireless", "wifi-iface");

const hostValues = { mac: "02:00:00:00:00:01", ip: "192.0.2.10" };
const ruleValues = { name: "拒绝来访", src: "vendor_lan" };
const redirectValues = {
  name: "网页服务",
  src: "vendor_wan",
  src_dport: "8080",
  dest_ip: "192.0.2.10",
  dest_port: "80",
};
const routeValues = { interface: "vendor_uplink", target: "192.0.2.0/24" };
const wifiValues = {
  device: "vendor_radio_5g",
  network: "vendor_lan",
  ssid: "家用网络",
  key: "a good password",
};

const moduleCoverage: readonly [ConfigurationModule, readonly string[]][] = [
  ["dhcp", ["host"]],
  ["firewall", ["rule", "redirect"]],
  ["network", ["interface", "route"]],
  ["wireless", ["wifi-iface"]],
  ["system", []],
  ["dropbear", []],
];

describe("routine section templates", () => {
  it.each(moduleCoverage)(
    "lists only routine %s section types",
    (module, types) => {
      expect(sectionTemplates(module).map((item) => item.type)).toEqual(types);
      expect(
        sectionTemplates(module).every((item) => item.module === module),
      ).toBe(true);
    },
  );

  it("has unique IDs, useful Chinese copy, and no internal section ID field", () => {
    const all = moduleCoverage.flatMap(([module]) => sectionTemplates(module));
    expect(all).toHaveLength(6);
    expect(new Set(all.map((item) => item.id)).size).toBe(all.length);
    for (const item of all) {
      expect(item.label).toMatch(/[\u4e00-\u9fff]/);
      expect(item.hint).toMatch(/[\u4e00-\u9fff]/);
      expect(item.deleteImpact).toMatch(/[\u4e00-\u9fff]/);
      expect(new Set(item.fields.map((field) => field.name)).size).toBe(
        item.fields.length,
      );
      expect(item.fields.map((field) => field.name)).not.toContain("id");
      for (const field of item.fields) {
        expect(field.label).toMatch(/[\u4e00-\u9fff]/);
        expect(field.hint).toMatch(/[\u4e00-\u9fff]/);
        if (field.widget === "select") {
          expect(field.options?.length).toBeGreaterThan(0);
          if (field.defaultValue !== undefined) {
            expect(field.options?.map((option) => option.value)).toContain(
              field.defaultValue,
            );
          }
        }
        if (field.widget === "reference") {
          expect(field.reference).toMatch(/^(interface|radio|zone)$/);
          expect(field.options).toBeUndefined();
        }
      }
    }
  });

  it("keeps new firewall rules disabled with a non-allow default", () => {
    expect(
      rule().fields.find((field) => field.name === "target"),
    ).toMatchObject({
      widget: "select",
      defaultValue: "REJECT",
      options: expect.arrayContaining([
        { value: "ACCEPT", label: expect.any(String) },
        { value: "DROP", label: expect.any(String) },
        { value: "REJECT", label: expect.any(String) },
      ]),
    });
    const enabled = rule().fields.find((field) => field.name === "enabled")!;
    expect(enabled.widget).toBe("select");
    expect(enabled.defaultValue).toBe("0");
    expect(enabled.hidden).not.toBe(true);
    expect(enabled.options?.map((option) => option.value)).toEqual(["0", "1"]);
  });

  it("hides fixed native values and masks credentials", () => {
    expect(
      redirect().fields.find((field) => field.name === "target"),
    ).toMatchObject({
      defaultValue: "DNAT",
      hidden: true,
    });
    expect(wifi().fields.find((field) => field.name === "mode")).toMatchObject({
      defaultValue: "ap",
      hidden: true,
    });
    expect(
      networkInterface().fields.find((field) => field.name === "password")
        ?.widget,
    ).toBe("password");
    expect(wifi().fields.find((field) => field.name === "key")?.widget).toBe(
      "password",
    );
    expect(
      host().fields.find((field) => field.name === "name")?.required,
    ).not.toBe(true);
    expect(
      redirect().fields.find((field) => field.name === "src")?.defaultValue,
    ).toBe("wan");
  });

  it("produces native option shapes with a separate internal section ID", () => {
    const forms: readonly [SectionTemplate, Record<string, string>][] = [
      [host(), hostValues],
      [rule(), ruleValues],
      [redirect(), redirectValues],
      [
        networkInterface(),
        { proto: "static", ipaddr: "192.0.2.1", netmask: "24" },
      ],
      [route(), routeValues],
      [wifi(), wifiValues],
    ];
    for (const [item, input] of forms) {
      const values = valuesFor(item, input);
      const fields = item.fields.flatMap((field) => {
        const value = values[field.name];
        return value ? [{ name: field.name, value }] : [];
      });
      const source = addNativeSection("", item.type, "panel_section_1", fields);
      const sections = nativeSections(source);
      expect(sections).toHaveLength(1);
      expect(sections[0]).toMatchObject({
        type: item.type,
        name: "panel_section_1",
      });
      expect(
        sections[0].fields.map(({ name, value, kind }) => ({
          name,
          value,
          kind,
        })),
      ).toEqual(fields.map((field) => ({ ...field, kind: "option" })));
    }
  });

  it("keeps the supported protocol and encryption choices exact", () => {
    const options = (item: SectionTemplate, name: string) =>
      item.fields
        .find((field) => field.name === name)
        ?.options?.map((option) => option.value);
    expect(options(networkInterface(), "proto")).toEqual([
      "dhcp",
      "static",
      "pppoe",
      "none",
    ]);
    expect(options(rule(), "proto")).toEqual([
      "tcp",
      "udp",
      "tcp udp",
      "icmp",
      "all",
    ]);
    expect(options(redirect(), "proto")).toEqual(["tcp", "udp", "tcp udp"]);
    expect(options(wifi(), "encryption")).toEqual([
      "psk2",
      "sae",
      "sae-mixed",
      "none",
    ]);
  });
});

describe("routine template validation", () => {
  it.each([
    [host, hostValues],
    [rule, ruleValues],
    [redirect, redirectValues],
    [networkInterface, {}],
    [route, routeValues],
    [wifi, wifiValues],
  ] satisfies readonly [() => SectionTemplate, Record<string, string>][])(
    "accepts a complete routine form %#",
    (getTemplate, values) => {
      const item = getTemplate();
      expect(templateErrors(item, valuesFor(item, values))).toEqual({});
    },
  );

  it.each([
    [host, ["mac", "ip"]],
    [rule, ["name", "src"]],
    [redirect, ["name", "src", "src_dport", "dest_ip", "dest_port"]],
    [route, ["interface", "target"]],
    [wifi, ["device", "network", "ssid", "key"]],
  ] satisfies readonly [() => SectionTemplate, readonly string[]][])(
    "reports required fields for form %#",
    (getTemplate, names) => {
      const item = getTemplate();
      const values = valuesFor(item);
      for (const name of names) values[name] = "";
      const errors = templateErrors(item, values);
      for (const name of names) expect(errors[name]).toMatch(/[\u4e00-\u9fff]/);
    },
  );

  it("does not apply metadata defaults or mutate the caller's values", () => {
    const values = Object.freeze({});
    expect(templateErrors(networkInterface(), values)).toHaveProperty("proto");
    expect(values).toEqual({});
    const valid = Object.freeze(valuesFor(wifi(), wifiValues));
    expect(templateErrors(wifi(), valid)).toEqual({});
    expect(valid.key).toBe("a good password");
  });

  it.each(["192.0.2.1", "0.0.0.0", "255.255.255.255"])(
    "accepts IPv4 address %s",
    (ip) => expect(templateErrors(host(), { ...hostValues, ip })).toEqual({}),
  );

  it.each([
    "256.0.0.1",
    "192.0.2",
    "1.2.3.4.5",
    "192.0.2.-1",
    "192.0.2.01",
    "2001:db8::1",
    "192.0.2.10/24",
    "192.0.2.10 trailing",
  ])("rejects malformed or non-IPv4 host address %s", (ip) =>
    expect(templateErrors(host(), { ...hostValues, ip }).ip).toBeTruthy(),
  );

  it.each(["02:ab:CD:00:01:fe", "02-ab-CD-00-01-fe"])(
    "accepts a six-octet MAC %s",
    (mac) => expect(templateErrors(host(), { ...hostValues, mac })).toEqual({}),
  );

  it.each([
    "02:00:00:00:00",
    "02:00:00:00:00:GG",
    "0200.0000.0001",
    "02:00-00:00:00:01",
    "02:00:00:00:00:01 02:00:00:00:00:02",
  ])("rejects malformed or multiple host MACs %s", (mac) =>
    expect(templateErrors(host(), { ...hostValues, mac }).mac).toBeTruthy(),
  );

  it("allows an optional hostname and checks it when supplied", () => {
    expect(templateErrors(host(), { ...hostValues, name: "" })).toEqual({});
    expect(
      templateErrors(host(), { ...hostValues, name: "printer-1.home" }),
    ).toEqual({});
    for (const name of [
      "a b",
      "-printer",
      "printer-",
      "a..b",
      "a".repeat(64),
    ]) {
      expect(templateErrors(host(), { ...hostValues, name }).name).toBeTruthy();
    }
  });

  it.each(["1", "65535", "1000-2000", "1000:2000"])(
    "accepts a forwarding port or range %s",
    (port) => {
      expect(
        templateErrors(
          redirect(),
          valuesFor(redirect(), {
            ...redirectValues,
            src_dport: port,
            dest_port: port,
          }),
        ),
      ).toEqual({});
    },
  );

  it.each([
    "0",
    "65536",
    "-1",
    "1.5",
    "eighty",
    "2000-1000",
    "80-",
    "-80",
    "80--90",
    "80-90-100",
    "80 443",
  ])("rejects malformed forwarding ports %s", (port) => {
    const errors = templateErrors(
      redirect(),
      valuesFor(redirect(), {
        ...redirectValues,
        src_dport: port,
        dest_port: port,
      }),
    );
    expect(errors.src_dport).toBeTruthy();
    expect(errors.dest_port).toBeTruthy();
  });

  it("accepts optional rule ports, lists, and ranges", () => {
    for (const dest_port of [
      "",
      "443",
      "80 443 1000-2000",
      "80,443",
      "1000:2000",
    ]) {
      expect(
        templateErrors(rule(), valuesFor(rule(), { ...ruleValues, dest_port })),
      ).toEqual({});
    }
    expect(
      templateErrors(
        rule(),
        valuesFor(rule(), { ...ruleValues, dest_port: "443,99999" }),
      ).dest_port,
    ).toBeTruthy();
  });

  it("requires static interface addresses and masks only optional PPPoE credentials", () => {
    const item = networkInterface();
    expect(
      templateErrors(item, valuesFor(item, { proto: "static" })),
    ).toMatchObject({
      ipaddr: expect.any(String),
      netmask: expect.any(String),
    });
    expect(templateErrors(item, valuesFor(item, { proto: "pppoe" }))).toEqual(
      {},
    );
    expect(templateErrors(item, valuesFor(item, { proto: "none" }))).toEqual(
      {},
    );
    expect(
      templateErrors(
        item,
        valuesFor(item, {
          proto: "static",
          ipaddr: "192.0.2.1",
          netmask: "255.255.255.0",
        }),
      ),
    ).toEqual({});
    expect(
      templateErrors(item, valuesFor(item, { ipaddr: "not an address" }))
        .ipaddr,
    ).toBeTruthy();
  });

  it.each(["0", "24", "32", "0.0.0.0", "255.255.255.0", "255.255.255.255"])(
    "accepts a contiguous netmask or prefix %s",
    (netmask) => {
      expect(
        templateErrors(
          networkInterface(),
          valuesFor(networkInterface(), {
            proto: "static",
            ipaddr: "192.0.2.1",
            netmask,
          }),
        ),
      ).toEqual({});
    },
  );

  it.each([
    "33",
    "-1",
    "24.5",
    "255.0.255.0",
    "255.255.255.1",
    "256.255.255.0",
  ])("rejects malformed or noncontiguous netmask %s", (netmask) =>
    expect(
      templateErrors(
        networkInterface(),
        valuesFor(networkInterface(), { netmask }),
      ).netmask,
    ).toBeTruthy(),
  );

  it.each([
    "192.0.2.1",
    "192.0.2.0/24",
    "0.0.0.0/0",
    "192.0.2.1/32",
    "192.0.2.1/24",
  ])("accepts an IPv4 route address or CIDR %s", (target) =>
    expect(
      templateErrors(route(), valuesFor(route(), { ...routeValues, target })),
    ).toEqual({}),
  );

  it.each([
    "192.0.2.0/33",
    "192.0.2.0/-1",
    "192.0.2.0/24/1",
    "256.0.2.0/24",
    "2001:db8::/64",
    "192.0.2.0/24.5",
  ])("rejects an invalid IPv4 route CIDR %s", (target) =>
    expect(
      templateErrors(route(), valuesFor(route(), { ...routeValues, target }))
        .target,
    ).toBeTruthy(),
  );

  it("validates an optional gateway and nonnegative integer metric", () => {
    for (const metric of ["", "0", "10"]) {
      expect(
        templateErrors(
          route(),
          valuesFor(route(), { ...routeValues, gateway: "192.0.2.1", metric }),
        ),
      ).toEqual({});
    }
    for (const metric of [
      "-1",
      "1.5",
      "NaN",
      "Infinity",
      "1e2",
      "9007199254740992",
    ]) {
      expect(
        templateErrors(route(), valuesFor(route(), { ...routeValues, metric }))
          .metric,
      ).toBeTruthy();
    }
    expect(
      templateErrors(
        route(),
        valuesFor(route(), { ...routeValues, gateway: "192.0.2.1/24" }),
      ).gateway,
    ).toBeTruthy();
  });

  it.each(["psk2", "sae", "sae-mixed"])(
    "requires a valid password for %s wireless encryption",
    (encryption) => {
      for (const key of ["", "short", "a".repeat(65)]) {
        expect(
          templateErrors(
            wifi(),
            valuesFor(wifi(), { ...wifiValues, encryption, key }),
          ).key,
        ).toBeTruthy();
      }
      for (const key of [
        "12345678",
        " pass word ",
        "a".repeat(63),
        "ab".repeat(32),
      ]) {
        expect(
          templateErrors(
            wifi(),
            valuesFor(wifi(), { ...wifiValues, encryption, key }),
          ),
        ).toEqual({});
      }
      expect(
        templateErrors(
          wifi(),
          valuesFor(wifi(), { ...wifiValues, encryption, key: "z".repeat(64) }),
        ).key,
      ).toBeTruthy();
    },
  );

  it("does not require or validate a password for an open wireless network", () => {
    for (const key of ["", "short"]) {
      expect(
        templateErrors(
          wifi(),
          valuesFor(wifi(), { ...wifiValues, encryption: "none", key }),
        ),
      ).toEqual({});
    }
  });

  it("checks SSID length in UTF-8 bytes and rejects newlines", () => {
    for (const ssid of ["a".repeat(32), "家".repeat(10)]) {
      expect(
        templateErrors(wifi(), valuesFor(wifi(), { ...wifiValues, ssid })),
      ).toEqual({});
    }
    for (const ssid of [
      "a".repeat(33),
      "家".repeat(11),
      "two\nlines",
      "two\rlines",
    ]) {
      expect(
        templateErrors(wifi(), valuesFor(wifi(), { ...wifiValues, ssid })).ssid,
      ).toBeTruthy();
    }
  });

  it.each([
    [networkInterface, "proto", "vendor-proto", {}],
    [rule, "proto", "gre", ruleValues],
    [rule, "target", "RETURN", ruleValues],
    [rule, "enabled", "true", ruleValues],
    [redirect, "proto", "icmp", redirectValues],
    [redirect, "target", "SNAT", redirectValues],
    [wifi, "mode", "sta", wifiValues],
    [wifi, "encryption", "vendor-security", wifiValues],
  ] satisfies readonly [
    () => SectionTemplate,
    string,
    string,
    Record<string, string>,
  ][])(
    "rejects unsupported choices for form %#",
    (getTemplate, name, value, baseValues) => {
      const item = getTemplate();
      expect(
        templateErrors(item, valuesFor(item, { ...baseValues, [name]: value }))[
          name
        ],
      ).toBeTruthy();
    },
  );

  it("does not limit vendor references to lan, wan, or radio0", () => {
    expect(
      templateErrors(
        rule(),
        valuesFor(rule(), { ...ruleValues, dest: "vendor_dmz" }),
      ),
    ).toEqual({});
    expect(
      templateErrors(
        redirect(),
        valuesFor(redirect(), { ...redirectValues, dest: "vendor_dmz" }),
      ),
    ).toEqual({});
    expect(templateErrors(route(), valuesFor(route(), routeValues))).toEqual(
      {},
    );
    expect(templateErrors(wifi(), valuesFor(wifi(), wifiValues))).toEqual({});
  });
});
