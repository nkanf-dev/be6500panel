import { describe, expect, it } from "vitest";
import type { ConfigurationModule } from "./contracts";
import {
  fieldSchema,
  sectionFields,
  sectionSchema,
  type FieldSchema,
} from "./field-schema";

const coverage: readonly [
  ConfigurationModule,
  string,
  string,
  FieldSchema["widget"],
][] = [
  ["network", "interface", "ipaddr", "text"],
  ["network", "interface", "proto", "select"],
  ["network", "interface", "password", "password"],
  ["network", "interface", "mtu", "number"],
  ["network", "interface", "peerdns", "boolean"],
  ["network", "device", "type", "select"],
  ["network", "device", "stp", "boolean"],
  ["network", "bridge-vlan", "vlan", "number"],
  ["network", "switch", "enable_vlan", "boolean"],
  ["network", "switch_vlan", "ports", "text"],
  ["network", "route", "target", "text"],
  ["network", "route6", "gateway", "text"],
  ["network", "rule", "lookup", "text"],
  ["network", "rule6", "priority", "number"],
  ["network", "globals", "ula_prefix", "text"],
  ["wireless", "wifi-device", "channel", "text"],
  ["wireless", "wifi-device", "txpower", "number"],
  ["wireless", "wifi-device", "disabled", "boolean"],
  ["wireless", "wifi-iface", "encryption", "select"],
  ["wireless", "wifi-iface", "key", "password"],
  ["wireless", "wifi-iface", "isolate", "boolean"],
  ["wireless", "wifi-iface", "auth_port", "number"],
  ["dhcp", "dnsmasq", "port", "number"],
  ["dhcp", "dnsmasq", "cachesize", "number"],
  ["dhcp", "dnsmasq", "domainneeded", "boolean"],
  ["dhcp", "dhcp", "start", "number"],
  ["dhcp", "dhcp", "limit", "number"],
  ["dhcp", "dhcp", "leasetime", "text"],
  ["dhcp", "dhcp", "ra", "select"],
  ["dhcp", "host", "ip", "text"],
  ["dhcp", "domain", "ip", "text"],
  ["dhcp", "odhcpd", "maindhcp", "boolean"],
  ["firewall", "defaults", "input", "select"],
  ["firewall", "defaults", "synflood_protect", "boolean"],
  ["firewall", "zone", "masq", "boolean"],
  ["firewall", "forwarding", "src", "text"],
  ["firewall", "rule", "target", "select"],
  ["firewall", "rule", "proto", "select"],
  ["firewall", "rule", "limit", "text"],
  ["firewall", "rule", "limit_burst", "number"],
  ["firewall", "redirect", "target", "select"],
  ["firewall", "nat", "target", "select"],
  ["firewall", "include", "path", "text"],
  ["firewall", "ipset", "maxelem", "number"],
  ["system", "system", "hostname", "text"],
  ["system", "system", "log_port", "number"],
  ["system", "system", "log_proto", "select"],
  ["system", "timeserver", "enabled", "boolean"],
  ["system", "led", "default", "boolean"],
  ["dropbear", "dropbear", "Port", "number"],
  ["dropbear", "dropbear", "PasswordAuth", "boolean"],
  ["dropbear", "dropbear", "IdleTimeout", "number"],
];

const knownSections: readonly [ConfigurationModule, string][] = [
  ["network", "interface"],
  ["network", "device"],
  ["network", "bridge-vlan"],
  ["network", "switch"],
  ["network", "switch_vlan"],
  ["network", "route"],
  ["network", "route6"],
  ["network", "rule"],
  ["network", "rule6"],
  ["network", "globals"],
  ["wireless", "wifi-device"],
  ["wireless", "wifi-iface"],
  ["dhcp", "dnsmasq"],
  ["dhcp", "dhcp"],
  ["dhcp", "host"],
  ["dhcp", "domain"],
  ["dhcp", "odhcpd"],
  ["dhcp", "cname"],
  ["dhcp", "boot"],
  ["dhcp", "relay"],
  ["dhcp", "srvhost"],
  ["dhcp", "mxhost"],
  ["firewall", "defaults"],
  ["firewall", "zone"],
  ["firewall", "forwarding"],
  ["firewall", "rule"],
  ["firewall", "redirect"],
  ["firewall", "nat"],
  ["firewall", "include"],
  ["firewall", "ipset"],
  ["system", "system"],
  ["system", "timeserver"],
  ["system", "led"],
  ["dropbear", "dropbear"],
];

function values(module: ConfigurationModule, section: string, field: string) {
  return fieldSchema(module, section, field).options?.map(
    (option) => option.value,
  );
}

describe("module and section aware UCI metadata", () => {
  it.each(coverage)(
    "describes %s / %s / %s as %s",
    (module, section, field, widget) => {
      const schema = fieldSchema(module, section, field);
      expect(schema.widget).toBe(widget);
      expect(schema.label).toMatch(/[\u4e00-\u9fff]/);
      expect(schema.hint).toMatch(/[\u4e00-\u9fff]/);
    },
  );

  it.each(knownSections)(
    "lists editable fields for %s / %s",
    (module, section) => {
      const schema = sectionSchema(module, section);
      expect(schema.label).toMatch(/[\u4e00-\u9fff]/);
      expect(schema.hint).toMatch(/[\u4e00-\u9fff]/);
      const fields = sectionFields(module, section);
      expect(fields.length).toBeGreaterThan(0);
      expect(new Set(fields).size).toBe(fields.length);
      for (const field of fields) {
        const metadata = fieldSchema(module, section, field);
        expect(metadata.label).toMatch(/[\u4e00-\u9fff]/);
        expect(metadata.hint).toMatch(/[\u4e00-\u9fff]/);
        if (metadata.widget === "select") {
          expect(metadata.options?.length).toBeGreaterThan(0);
          const options = metadata.options!;
          expect(new Set(options.map((option) => option.value)).size).toBe(
            options.length,
          );
          expect(options.every((option) => option.label.length > 0)).toBe(true);
        } else {
          expect(metadata.options).toBeUndefined();
        }
        if (metadata.widget === "number") {
          expect(metadata.step).toBe(1);
          if (metadata.min !== undefined && metadata.max !== undefined) {
            expect(metadata.max).toBeGreaterThanOrEqual(metadata.min);
          }
        }
      }
    },
  );

  it("distinguishes matching field names by module and section", () => {
    expect(fieldSchema("dhcp", "dhcp", "limit").widget).toBe("number");
    expect(fieldSchema("firewall", "rule", "limit").widget).toBe("text");
    expect(fieldSchema("network", "route", "target").widget).toBe("text");
    expect(fieldSchema("firewall", "rule", "target").widget).toBe("select");
    expect(fieldSchema("wireless", "wifi-device", "type").widget).toBe("text");
    expect(fieldSchema("network", "device", "type").widget).toBe("select");
    expect(fieldSchema("network", "interface", "disabled").widget).toBe(
      "boolean",
    );
    expect(fieldSchema("network", "vendor", "disabled").widget).toBe("text");
    expect(values("network", "interface", "proto")).toContain("dhcp");
    expect(values("firewall", "rule", "proto")).not.toContain("dhcp");
    expect(values("firewall", "rule", "proto")).toContain("tcp");
    expect(values("network", "interface", "proto")).not.toContain("tcp");
    expect(sectionSchema("network", "rule")).not.toEqual(
      sectionSchema("firewall", "rule"),
    );
  });

  it.each([
    ["network", "interface", "ipaddr"],
    ["network", "interface", "ip6addr"],
    ["network", "interface", "netmask"],
    ["network", "interface", "dns"],
    ["wireless", "wifi-iface", "maclist"],
    ["dhcp", "host", "mac"],
    ["dhcp", "dnsmasq", "server"],
    ["dhcp", "dhcp", "dhcp_option"],
    ["firewall", "rule", "src_ip"],
    ["firewall", "rule", "dest_ip"],
    ["firewall", "rule", "src_port"],
    ["firewall", "redirect", "dest_port"],
    ["system", "timeserver", "server"],
  ] satisfies readonly [ConfigurationModule, string, string][])(
    "preserves address, list and range syntax as text: %s / %s / %s",
    (module, section, field) => {
      expect(fieldSchema(module, section, field).widget).toBe("text");
    },
  );

  it("uses number inputs only for known scalar numbers with relevant bounds", () => {
    expect(fieldSchema("dropbear", "dropbear", "Port")).toMatchObject({
      min: 1,
      max: 65535,
      step: 1,
    });
    expect(fieldSchema("dhcp", "dnsmasq", "port")).toMatchObject({
      min: 0,
      max: 65535,
    });
    expect(fieldSchema("dhcp", "dhcp", "start")).toMatchObject({
      min: 0,
      max: 65535,
    });
    expect(fieldSchema("dropbear", "dropbear", "IdleTimeout")).toMatchObject({
      min: 0,
      step: 1,
    });
    expect(fieldSchema("wireless", "wifi-device", "channel").widget).toBe(
      "text",
    );
    expect(fieldSchema("dhcp", "dhcp", "leasetime").widget).toBe("text");
    expect(fieldSchema("network", "route", "table").widget).toBe("text");
  });

  it.each([
    ["wireless", "wifi-iface", "key"],
    ["wireless", "wifi-iface", "key1"],
    ["wireless", "wifi-iface", "auth_secret"],
    ["wireless", "wifi-iface", "acct_secret"],
    ["network", "interface", "password"],
  ] satisfies readonly [ConfigurationModule, string, string][])(
    "masks known credentials: %s / %s / %s",
    (module, section, field) => {
      expect(fieldSchema(module, section, field).widget).toBe("password");
    },
  );

  it("keeps select values faithful to native UCI tokens", () => {
    expect(values("network", "interface", "proto")).toEqual(
      expect.arrayContaining([
        "static",
        "dhcp",
        "dhcpv6",
        "pppoe",
        "none",
        "l2tp",
        "pptp",
      ]),
    );
    expect(values("wireless", "wifi-iface", "encryption")).toEqual(
      expect.arrayContaining([
        "none",
        "psk",
        "psk2",
        "psk-mixed",
        "psk2+ccmp",
        "psk2+tkip+ccmp",
        "sae",
        "sae-mixed",
        "owe",
        "wpa2",
      ]),
    );
    expect(values("wireless", "wifi-iface", "encryption")).not.toContain(
      "WPA2-PSK",
    );
    expect(values("firewall", "defaults", "input")).toEqual([
      "ACCEPT",
      "REJECT",
      "DROP",
    ]);
    expect(values("firewall", "rule", "proto")).toContain("tcp udp");
    expect(values("firewall", "rule", "proto")).not.toContain("tcp+udp");
    expect(values("firewall", "redirect", "target")).toEqual(["DNAT", "SNAT"]);
    expect(values("firewall", "rule", "target")).not.toContain("DNAT");
    expect(values("network", "rule", "action")).not.toContain("lookup");
    expect(values("network", "rule", "action")).toContain("unicast");
  });

  it("does not infer unknown vendor types or apply known fields outside their context", () => {
    for (const field of [
      "vendor_enabled",
      "vendor_port",
      "vendor_timeout",
      "password",
      "constructor",
      "__proto__",
      "toString",
    ]) {
      expect(fieldSchema("system", "vendor", field)).toMatchObject({
        label: field,
        widget: "text",
      });
      expect(fieldSchema("system", "vendor", field).options).toBeUndefined();
    }
    expect(fieldSchema("network", "interface", "vendor_enabled").widget).toBe(
      "text",
    );
    expect(fieldSchema("network", "interface", "vendor_port").widget).toBe(
      "text",
    );
    expect(fieldSchema("network", "interface", "encryption").widget).toBe(
      "text",
    );
    expect(fieldSchema("system", "system", "PasswordAuth").widget).toBe("text");
    expect(fieldSchema("dropbear", "dropbear", "Port").widget).toBe("number");
    expect(fieldSchema("dropbear", "dropbear", "port").widget).toBe("text");
    expect(sectionFields("system", "vendor")).toEqual([]);
    expect(sectionFields("system", "constructor")).toEqual([]);
    expect(sectionSchema("system", "vendor")).toMatchObject({
      label: "vendor",
    });
    expect(sectionSchema("system", "vendor").hint).toMatch(/[\u4e00-\u9fff]/);
    expect(sectionFields("dropbear", "dropbear")).toContain("Port");
    expect(sectionFields("dropbear", "dropbear")).not.toContain("port");
    expect(fieldSchema("dropbear", "dropbear", "verbose").widget).toBe("text");
  });
});
