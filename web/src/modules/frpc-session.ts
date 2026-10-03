import type { FrpcPlanInput } from "../lib/contracts";
import type { FrpcDocument, FrpcTokenIntent } from "./frpc-document";
import { createFrpcProxy } from "./frpc-tunnel-editor";

export interface FrpcFormSession {
  service: "frpc";
  /** Generation of the accepted document on which local intent is based. */
  generation?: number;
  document?: FrpcDocument;
  input: FrpcPlanInput;
  token: FrpcTokenIntent;
  dirty: boolean;
}
let buffer: FrpcFormSession | undefined;
let epoch = 0;
let revision = 0;
const listeners = new Set<() => void>();
export const frpcFormSessionRevision = () => revision;
export const frpcFormSessionEpoch = () => epoch;
export const emptyFrpcFormSession = (): FrpcFormSession => ({
  service: "frpc",
  input: {
    serverAddress: "",
    serverPort: 7000,
    tls: true,
    transport: "tcp",
    proxies: [createFrpcProxy(1)],
  },
  token: { mode: "preserve", value: "" },
  dirty: false,
});
export const readFrpcFormSession = () =>
  structuredClone(buffer ?? emptyFrpcFormSession());
export function writeFrpcFormSession(
  next: FrpcFormSession,
  observedEpoch: number,
) {
  if (observedEpoch === epoch) {
    buffer = structuredClone(next);
    revision++;
  }
}
/** Accepted writes must not replace an edit made after the request, even on another mount. */
export function acceptFrpcFormSession(
  next: FrpcFormSession,
  observedEpoch: number,
  observedRevision: number,
) {
  if (observedEpoch !== epoch || observedRevision !== revision) return false;
  writeFrpcFormSession(next, observedEpoch);
  return true;
}
export function subscribeFrpcFormSessionClear(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
/** Root calls on logout/expired auth. Retained text and credentials exist only in session memory. */
export function clearFrpcFormSession() {
  buffer = undefined;
  epoch++;
  revision++;
  for (const listener of listeners) listener();
}
if (typeof window !== "undefined") {
  window.addEventListener("be6500panel:unauthorized", clearFrpcFormSession);
  window.addEventListener("beforeunload", (event) => {
    if (!buffer?.dirty) return;
    event.preventDefault();
    event.returnValue = "";
  });
}
