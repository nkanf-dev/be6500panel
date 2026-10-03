export interface FieldEvidence {
  /** Rootfs-relative path; static/ and live-inspection/ use the research tree, docs/ uses this repository. */
  source: string;
  line: number;
  endLine?: number;
  firmware: "Xiaomi RN02 1.0.43" | "Xiaomi RN02 1.0.64";
  /** The concrete fact supported by this location, not the current setting. */
  fact: string;
  /** Underlying stock binary when the cited text is an extracted ELF table. */
  artifact?: { source: string; sha256: string; offset?: number };
}

export interface FieldHelp {
  /** Optional compact correction where the generic schema hint conflicts with vendor behavior. */
  summary?: string;
  description: string;
  /** Only a fallback demonstrated in source, never a sampled live value. */
  defaultValue?: string;
  unit?: string;
  /** Source or protocol constraint; does not replace input min/max. */
  range?: string;
  dependencies?: readonly string[];
  impact: string;
  flags?: readonly (
    | "generated"
    | "hardware-dependent"
    | "credential"
    | "legacy"
    | "version-dependent"
  )[];
  evidence: readonly FieldEvidence[];
  /** Explicit bounded search result where the stock source does not prove this field. */
  discovery?: string;
}

export type ModuleFieldHelp = Readonly<
  Record<string, Readonly<Record<string, FieldHelp>>>
>;
