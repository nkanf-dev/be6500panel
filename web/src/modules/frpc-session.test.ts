import { afterEach, describe, expect, it, vi } from "vitest";
import {
  acceptFrpcFormSession,
  frpcFormSessionRevision,
  clearFrpcFormSession,
  emptyFrpcFormSession,
  frpcFormSessionEpoch,
  readFrpcFormSession,
  subscribeFrpcFormSessionClear,
  writeFrpcFormSession,
} from "./frpc-session";

afterEach(clearFrpcFormSession);
describe("private frpc form session", () => {
  it("retains dirty intent against its original generation without browser storage", () => {
    const storage = vi.spyOn(Storage.prototype, "setItem");
    const session = {
      ...emptyFrpcFormSession(),
      generation: 7,
      dirty: true,
      token: { mode: "replace" as const, value: "synthetic-private" },
    };
    writeFrpcFormSession(session, frpcFormSessionEpoch());
    const restored = readFrpcFormSession();
    expect(restored).toEqual(session);
    restored.input.serverAddress = "changed";
    expect(readFrpcFormSession().input.serverAddress).toBe("");
    expect(storage).not.toHaveBeenCalled();
  });
  it("logout clears mounted forms and rejects late writes from the previous session", () => {
    const epoch = frpcFormSessionEpoch();
    const session = { ...emptyFrpcFormSession(), dirty: true };
    writeFrpcFormSession(session, epoch);
    const listener = vi.fn();
    const unsubscribe = subscribeFrpcFormSessionClear(listener);
    clearFrpcFormSession();
    writeFrpcFormSession(session, epoch);
    expect(readFrpcFormSession().dirty).toBe(false);
    expect(listener).toHaveBeenCalledOnce();
    unsubscribe();
  });
  it("does not let an old save response overwrite a newer draft from a different page mount", () => {
    const epoch = frpcFormSessionEpoch();
    writeFrpcFormSession({ ...emptyFrpcFormSession(), dirty: true }, epoch);
    const revision = frpcFormSessionRevision();
    const later = {
      ...emptyFrpcFormSession(),
      dirty: true,
      token: { mode: "replace" as const, value: "synthetic-later" },
    };
    writeFrpcFormSession(later, epoch);
    expect(acceptFrpcFormSession(emptyFrpcFormSession(), epoch, revision)).toBe(
      false,
    );
    expect(readFrpcFormSession()).toEqual(later);
  });
  it("clears private readback and intent on unauthorized", () => {
    writeFrpcFormSession(
      { ...emptyFrpcFormSession(), dirty: true },
      frpcFormSessionEpoch(),
    );
    window.dispatchEvent(new Event("be6500panel:unauthorized"));
    expect(readFrpcFormSession().dirty).toBe(false);
  });
});
