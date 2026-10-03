import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";
import ts from "typescript";

const webRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(webRoot, "..");
const firmwareRoot = process.env.BE6500_FIRMWARE_ROOT;
const researchRoot = process.env.BE6500_RESEARCH_ROOT;
if (!firmwareRoot || !researchRoot) {
  throw new Error(
    "Set BE6500_FIRMWARE_ROOT and BE6500_RESEARCH_ROOT to the read-only evidence trees.",
  );
}

// Catalog modules contain presentation data and type-only imports. Compile with
// the project's TypeScript, not a separate runtime or a kernel-side dependency.
const modules = [
  "network",
  "wireless",
  "dhcp",
  "firewall",
  "system",
  "dropbear",
];
const sources = new Set();
const coverage = {};
const catalogs = {};
for (const module of modules) {
  const path = resolve(
    webRoot,
    "src/components/configuration/field-help",
    `${module}.ts`,
  );
  const { outputText } = ts.transpileModule(readFileSync(path, "utf8"), {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
    },
  });
  const exports = {};
  runInNewContext(outputText, { exports }, { filename: path, timeout: 1000 });
  const catalog = exports[`${module}FieldHelp`];
  catalogs[module] = catalog;
  const entries = Object.entries(catalog).flatMap(([section, fields]) =>
    Object.entries(fields).map(([field, help]) => ({ section, field, help })),
  );
  coverage[module] = {
    fields: entries.length,
    discoveries: entries
      .filter(({ help }) => help.discovery)
      .map(({ section, field }) => `${section}/${field}`),
  };
  for (const { help } of entries)
    for (const item of help.evidence) {
      sources.add(item.source);
      if (item.artifact) sources.add(item.artifact.source);
    }
}

const manifest = {};
for (const source of [...sources].sort()) {
  if (
    source.startsWith("/") ||
    source.includes("..") ||
    source.includes("\\")
  ) {
    throw new Error(`Evidence path must be relative and contained: ${source}`);
  }
  const root = source.startsWith("docs/")
    ? repositoryRoot
    : source.startsWith("static/") || source.startsWith("live-inspection/")
      ? researchRoot
      : firmwareRoot;
  const path = resolve(root, source);
  if (!path.startsWith(resolve(root) + sep))
    throw new Error(`Evidence escaped its tree: ${source}`);
  const content = readFileSync(path);
  manifest[source] = {
    sha256: createHash("sha256").update(content).digest("hex"),
    bytes: content.length,
    lines: content.toString("utf8").split("\n").length,
  };
}
writeFileSync(
  resolve(
    webRoot,
    "src/components/configuration/field-help/evidence-manifest.json",
  ),
  JSON.stringify(manifest, null, 2) + "\n",
);
console.log(JSON.stringify({ coverage, sources: sources.size }, null, 2));

const document = [
  "# Configuration field help: firmware evidence",
  "",
  "This catalog describes every field in the panel's canonical inventory. All fields remain editable. Generated, legacy, hardware-dependent, and credential flags describe behavior; they do not add read-only controls.",
  "",
  "## Evidence boundaries",
  "",
  `- Baseline: Xiaomi RN02 firmware 1.0.43, read-only evidence root \`${firmwareRoot}\` (\`BE6500_FIRMWARE_ROOT\`).`,
  `- Decompiled vendor Lua: \`${resolve(researchRoot, "static/lua-analysis/decompiled")}\` (under \`BE6500_RESEARCH_ROOT\`). These are research artifacts derived from baseline Lua bytecode, not current live settings.`,
  `- Public 1.0.64 service/protocol scripts: \`${resolve(researchRoot, "live-inspection/field-help-live-1.0.64")}\`. No private \`/etc/config\` values are copied here.`,
  "- Public script comparison: Dropbear, system, DHCPv4 and PPP scripts have the same SHA in 1.0.43 and the inspected 1.0.64 scripts. dnsmasq and DHCPv6 scripts differ. No static protocol script exists in either snapshot; static interface parsing belongs to netifd.",
  "- A source location proves the stated fact only. A parsed option name in an ELF table does not by itself prove runtime behavior. Search results list exactly which consumers were checked and which finer behavior is not yet proved.",
  "- Defaults below are explicit source fallbacks, not sampled settings. Units and ranges describe the named consumer or protocol, not blanket device support. Existing form bounds are unchanged.",
  "- Stock Dropbear init gates are not the panel's independently managed rescue SSH process. Help does not turn these fields into read-only controls.",
  "",
  "## Reproduce source integrity checks",
  "",
  "From `web/`:",
  "",
  "```sh",
  `BE6500_FIRMWARE_ROOT=${JSON.stringify(firmwareRoot)} BE6500_RESEARCH_ROOT=${JSON.stringify(researchRoot)} node scripts/build-field-evidence-manifest.mjs`,
  `BE6500_FIRMWARE_ROOT=${JSON.stringify(firmwareRoot)} BE6500_RESEARCH_ROOT=${JSON.stringify(researchRoot)} npm test -- src/components/configuration/field-help.test.ts src/components/configuration/NativeFields-help.test.tsx`,
  "```",
  "",
  "The manifest stores source SHA-256 hashes, byte sizes and line counts, not private source settings. Tests cover inventory equality, path/line bounds, extracted-binary offset bounds, firmware labels, credential help, editable native values and source hashes. These checks verify source integrity, not every semantic claim; reviewers also inspect the actual consumer branches. The optional mounted-source check is skipped when the evidence trees are not available.",
  "",
  "## Module coverage",
  "",
  "| Module | Catalog fields | Fields with explicit finer-evidence gaps |",
  "| --- | ---: | ---: |",
  ...modules.map(
    (module) =>
      `| ${module} | ${coverage[module].fields} | ${coverage[module].discoveries.length} |`,
  ),
  "",
  `Total: ${modules.reduce((sum, module) => sum + coverage[module].fields, 0)} fields; ${sources.size} referenced source files.`,
  "",
];
for (const module of modules) {
  document.push(`## ${module}`, "");
  for (const [section, fields] of Object.entries(catalogs[module])) {
    document.push(`### ${section}`, "");
    for (const [field, help] of Object.entries(fields)) {
      document.push(
        `#### ${module}/${section}/${field}`,
        "",
        help.description,
        "",
      );
      if (help.summary) document.push(`- Compact help: ${help.summary}`);
      if (help.defaultValue !== undefined)
        document.push(`- Source fallback: ${help.defaultValue}`);
      if (help.unit) document.push(`- Unit: ${help.unit}`);
      if (help.range) document.push(`- Format / range: ${help.range}`);
      if (help.flags?.length)
        document.push(`- Flags: ${help.flags.join(", ")}`);
      for (const dependency of help.dependencies ?? [])
        document.push(`- Condition: ${dependency}`);
      document.push(`- Apply impact: ${help.impact}`);
      if (help.discovery) document.push(`- Trace result: ${help.discovery}`);
      for (const item of help.evidence) {
        document.push(
          `- Evidence: \`${item.source}:${item.line}${item.endLine ? `–${item.endLine}` : ""}\` (${item.firmware}) — ${item.fact}`,
        );
        if (item.artifact)
          document.push(
            `  - Stock binary: \`${item.artifact.source}\`; SHA-256 \`${item.artifact.sha256}\`${item.artifact.offset !== undefined ? `; offset ${item.artifact.offset}` : ""}.`,
          );
      }
      document.push("");
    }
  }
}
writeFileSync(
  resolve(repositoryRoot, "docs/field-help-evidence.md"),
  document.join("\n").trimEnd() + "\n",
);
