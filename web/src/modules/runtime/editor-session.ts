import type { RuntimeService } from "../../lib/contracts";

export interface RuntimeEditorBuffer {
  config: string;
  original: string;
  /** Generation observed when the saved baseline was loaded. Never rebase dirty text silently. */
  generation?: number;
}

const buffers = new Map<RuntimeService, RuntimeEditorBuffer>();
const clearListeners = new Set<() => void>();
let epoch = 0;
export const runtimeEditorSessionEpoch = () => epoch;
export const emptyRuntimeEditorBuffer = (): RuntimeEditorBuffer => ({
  config: "",
  original: "",
});
export const readRuntimeEditorBuffer = (
  service: RuntimeService,
): RuntimeEditorBuffer => ({
  ...(buffers.get(service) ?? emptyRuntimeEditorBuffer()),
});
export function writeRuntimeEditorBuffer(
  service: RuntimeService,
  buffer: RuntimeEditorBuffer,
  observedEpoch: number,
) {
  if (observedEpoch !== epoch) return;
  if (buffer.generation !== undefined && buffer.config !== buffer.original)
    buffers.set(service, { ...buffer });
  else buffers.delete(service);
}
/** Clear only the draft included in this accepted write, never a newer edit. */
export function clearAcceptedRuntimeEditorBuffer(
  service: RuntimeService,
  accepted: RuntimeEditorBuffer,
  observedEpoch: number,
) {
  if (observedEpoch !== epoch) return;
  const current = buffers.get(service);
  if (
    current?.config === accepted.config &&
    current.original === accepted.original &&
    current.generation === accepted.generation
  )
    buffers.delete(service);
}
export function subscribeRuntimeEditorClear(listener: () => void) {
  clearListeners.add(listener);
  return () => {
    clearListeners.delete(listener);
  };
}
/** Root calls this on logout. Unauthorized also clears retained private edits. No browser storage is used. */
export function clearRuntimeEditorSession() {
  buffers.clear();
  epoch++;
  for (const listener of clearListeners) listener();
}
if (typeof window !== "undefined") {
  window.addEventListener(
    "be6500panel:unauthorized",
    clearRuntimeEditorSession,
  );
  window.addEventListener("beforeunload", (event) => {
    if (buffers.size === 0) return;
    event.preventDefault();
    event.returnValue = "";
  });
}
