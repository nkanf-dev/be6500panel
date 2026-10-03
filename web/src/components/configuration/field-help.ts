import type { ConfigurationModule } from "./contracts";
import type { FieldHelp, ModuleFieldHelp } from "./field-help/types";
import { networkFieldHelp } from "./field-help/network";
import { wirelessFieldHelp } from "./field-help/wireless";
import { dhcpFieldHelp } from "./field-help/dhcp";
import { firewallFieldHelp } from "./field-help/firewall";
import { systemFieldHelp } from "./field-help/system";
import { dropbearFieldHelp } from "./field-help/dropbear";

export const firmwareContext =
  "Xiaomi RN02 1.0.43 静态固件；部分公共脚本已与 1.0.64 对照，尚未逐项验证 1.0.64 的完整行为。";

export const fieldHelpCatalog: Readonly<
  Record<ConfigurationModule, ModuleFieldHelp>
> = {
  network: networkFieldHelp,
  wireless: wirelessFieldHelp,
  dhcp: dhcpFieldHelp,
  firewall: firewallFieldHelp,
  system: systemFieldHelp,
  dropbear: dropbearFieldHelp,
};

export function fieldHelp(
  module: ConfigurationModule,
  section: string,
  field: string,
): FieldHelp | undefined {
  const catalog = fieldHelpCatalog[module];
  if (!Object.hasOwn(catalog, section)) return undefined;
  const fields = catalog[section];
  return Object.hasOwn(fields, field) ? fields[field] : undefined;
}
