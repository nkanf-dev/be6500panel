import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { errorMessage, runRequest } from "../../lib/api";
import { jsonResponse } from "../production-fixtures.test-data";
import {
  deviceAnnotationsAPI,
  type DeviceAnnotations,
  type SaveDeviceAnnotation,
} from "./annotations-api";
import {
  canonicalMAC,
  DEVICE_LABELS_CHANGED,
  DeviceLabel,
  DeviceLabelsProvider,
  deviceDisplayName,
  useDeviceLabels,
} from "./device-labels";

const mac = "AA:BB:CC:DD:EE:FF";
const dashedMAC = "aa-bb-cc-dd-ee-ff";
const annotation = {
  label: "Custom name",
  note: "Private note",
  tags: ["family"],
};
const initial: DeviceAnnotations = {
  revision: 3,
  devices: { [mac]: annotation },
};
const changed: DeviceAnnotations = {
  revision: 4,
  devices: {
    [mac]: {
      label: "New name",
      note: "Changed note",
      tags: ["work", "laptop"],
    },
  },
};
const saveInput: SaveDeviceAnnotation = {
  mac: dashedMAC,
  label: "New name",
  note: "Changed note",
  tags: ["work", "laptop"],
  expectedRevision: initial.revision,
};
let labels: ReturnType<typeof useDeviceLabels>;
function Consumer({ hostname = "System name" }: { hostname?: string }) {
  labels = useDeviceLabels();
  const [draft, setDraft] = useState("Draft name");
  const [saveError, setSaveError] = useState("");
  return (
    <>
      <span data-testid="list-alias">
        <DeviceLabel mac={mac} hostname={hostname} />
      </span>
      <span data-testid="detail-alias">
        <DeviceLabel mac={dashedMAC} hostname={hostname} />
      </span>
      <span data-testid="revision">{labels.revision}</span>
      <span data-testid="loading">{String(labels.loading)}</span>
      <span data-testid="load-error">
        {labels.error ? errorMessage(labels.error) : "none"}
      </span>
      <label>
        Device name
        <input
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
        />
      </label>
      <button
        onClick={() => {
          void labels
            .save({
              ...saveInput,
              label: draft,
              expectedRevision: labels.revision,
            })
            .catch((cause) => setSaveError(errorMessage(cause)));
        }}
      >
        Save name
      </button>
      <span data-testid="save-error">{saveError}</span>
    </>
  );
}
const renderLabels = (data?: DeviceAnnotations) =>
  render(
    <DeviceLabelsProvider initial={data}>
      <Consumer />
    </DeviceLabelsProvider>,
  );
const expectAliases = (name: string) => {
  expect(screen.getByTestId("list-alias")).toHaveTextContent(name);
  expect(screen.getByTestId("detail-alias")).toHaveTextContent(name);
};
const save = async (input = saveInput) => {
  await act(async () => {
    await labels.save(input);
  });
};
const refresh = () => {
  act(() => labels.refresh());
};
const storageSpies = () => [
  vi.spyOn(Storage.prototype, "getItem"),
  vi.spyOn(Storage.prototype, "setItem"),
  vi.spyOn(Storage.prototype, "removeItem"),
  vi.spyOn(Storage.prototype, "clear"),
];
const pendingFetch = () => {
  const pending: { signal: AbortSignal; resolve: (value: Response) => void }[] =
    [];
  const fetch = vi.fn(
    (_url: string, init: RequestInit) =>
      new Promise<Response>((resolve, reject) => {
        const signal = init.signal as AbortSignal;
        pending.push({ signal, resolve });
        signal.addEventListener(
          "abort",
          () => reject(new DOMException("Aborted", "AbortError")),
          { once: true },
        );
      }),
  );
  vi.stubGlobal("fetch", fetch);
  return { fetch, pending };
};
beforeEach(() => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(jsonResponse(initial)));
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("device identity and display names", () => {
  it.each(["aa:bb:cc:dd:ee:ff", "aa-bb-cc-dd-ee-ff", " AA:BB:CC:DD:EE:FF "])(
    "canonicalizes a valid MAC %s",
    (value) => expect(canonicalMAC(value)).toBe(mac),
  );

  it.each([
    "",
    "192.0.2.20",
    "2001:db8::20",
    "not-a-mac",
    "AA:BB:CC:DD:EE",
    "AA:BB:CC:DD:EE:GG",
    "AABBCCDDEEFF",
  ])("rejects IP-only or invalid identity %s", (value) =>
    expect(canonicalMAC(value)).toBeUndefined(),
  );

  it("prefers a trimmed custom name, then system name, then canonical MAC", () => {
    expect(deviceDisplayName(dashedMAC, "System name", initial.devices)).toBe(
      "Custom name",
    );
    expect(
      deviceDisplayName(dashedMAC, " System name ", {
        [mac]: { ...annotation, label: "  " },
      }),
    ).toBe("System name");
    expect(deviceDisplayName(dashedMAC, "  ")).toBe(mac);
    expect(
      deviceDisplayName(mac, undefined, {
        [mac]: { ...annotation, label: "  Named device  " },
      }),
    ).toBe("Named device");
    expect(deviceDisplayName("", "")).toBe("未命名设备");
  });

  it("renders the system fallback outside the provider but never allows an unconnected save", async () => {
    render(<Consumer />);
    expectAliases("System name");
    await expect(labels.save(saveInput)).rejects.toMatchObject({
      code: "annotations_not_mounted",
    });
    expect(fetch).not.toHaveBeenCalled();
  });
});

describe("session-only device labels provider", () => {
  it("loads annotations once and never auto-writes draft or hostname changes", async () => {
    const storage = storageSpies();
    const view = renderLabels();
    expect(screen.getByTestId("loading")).toHaveTextContent("true");
    await waitFor(() => expectAliases("Custom name"));
    expect(screen.getByTestId("revision")).toHaveTextContent("3");
    expect(screen.getByTestId("loading")).toHaveTextContent("false");
    fireEvent.change(screen.getByRole("textbox", { name: "Device name" }), {
      target: { value: "Unsaved name" },
    });
    view.rerender(
      <DeviceLabelsProvider>
        <Consumer hostname="Updated system name" />
      </DeviceLabelsProvider>,
    );
    expectAliases("Custom name");
    expect(fetch).toHaveBeenCalledOnce();
    expect(fetch).toHaveBeenCalledWith(
      "/api/devices/annotations",
      expect.objectContaining({
        method: "GET",
        credentials: "same-origin",
        signal: expect.any(AbortSignal),
      }),
    );
    expect(vi.mocked(fetch).mock.calls[0][1]).not.toHaveProperty("body");
    for (const spy of storage) expect(spy).not.toHaveBeenCalled();
  });

  it("writes only on explicit save with canonical MAC, exact metadata and expectedRevision", async () => {
    const storage = storageSpies();
    vi.mocked(fetch).mockResolvedValue(jsonResponse(changed));
    renderLabels(initial);
    expect(fetch).not.toHaveBeenCalled();
    fireEvent.change(screen.getByRole("textbox", { name: "Device name" }), {
      target: { value: "New name" },
    });
    expect(fetch).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Save name" }));
    await waitFor(() => expectAliases("New name"));
    expect(fetch).toHaveBeenCalledOnce();
    expect(fetch).toHaveBeenCalledWith(
      "/api/devices/annotations",
      expect.objectContaining({
        method: "POST",
        credentials: "same-origin",
        headers: {
          Accept: "application/json",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ ...saveInput, mac }),
      }),
    );
    expect(screen.getByTestId("revision")).toHaveTextContent("4");
    expect(labels.annotations[mac]).toEqual(changed.devices[mac]);
    for (const spy of storage) expect(spy).not.toHaveBeenCalled();
  });

  it("publishes alias invalidation and ignores older revisions after a save", async () => {
    const invalidated = vi.fn();
    window.addEventListener(DEVICE_LABELS_CHANGED, invalidated);
    try {
      vi.mocked(fetch)
        .mockResolvedValueOnce(jsonResponse(changed))
        .mockResolvedValueOnce(
          jsonResponse({
            revision: 2,
            devices: { [mac]: { ...annotation, label: "Outdated name" } },
          }),
        )
        .mockResolvedValueOnce(jsonResponse({ revision: 5, devices: {} }));
      renderLabels(initial);
      await save();
      expectAliases("New name");
      expect(invalidated).toHaveBeenCalledOnce();
      refresh();
      await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2));
      await waitFor(() =>
        expect(screen.getByTestId("loading")).toHaveTextContent("false"),
      );
      expectAliases("New name");
      expect(screen.getByTestId("revision")).toHaveTextContent("4");
      refresh();
      await waitFor(() => expectAliases("System name"));
      expect(screen.getByTestId("revision")).toHaveTextContent("5");
      expect(labels.annotations).toEqual({});
      expect(invalidated).toHaveBeenCalledTimes(3);
    } finally {
      window.removeEventListener(DEVICE_LABELS_CHANGED, invalidated);
    }
  });

  it("refuses an IP or invalid MAC save before making a request", async () => {
    renderLabels(initial);
    for (const invalid of ["192.0.2.20", "2001:db8::20", "not-a-mac", ""]) {
      await expect(
        labels.save({ ...saveInput, mac: invalid }),
      ).rejects.toMatchObject({ code: "invalid_mac" });
    }
    expectAliases("Custom name");
    expect(fetch).not.toHaveBeenCalled();
  });

  it("retains saved aliases on a revision conflict and does not retry the mutation", async () => {
    vi.mocked(fetch).mockResolvedValue(
      jsonResponse(
        {
          error: {
            code: "revision_conflict",
            message: "Refresh before saving",
          },
        },
        409,
      ),
    );
    renderLabels(initial);
    await expect(labels.save(saveInput)).rejects.toMatchObject({
      code: "revision_conflict",
      status: 409,
    });
    expectAliases("Custom name");
    expect(screen.getByTestId("revision")).toHaveTextContent("3");
    expect(fetch).toHaveBeenCalledOnce();
  });

  it("does not retry an explicit save after a transient server failure", async () => {
    vi.mocked(fetch).mockImplementation(() =>
      Promise.resolve(
        jsonResponse(
          {
            error: {
              code: "save_failed",
              message: "Annotation store unavailable",
            },
          },
          500,
        ),
      ),
    );
    renderLabels(initial);
    await expect(labels.save(saveInput)).rejects.toMatchObject({
      code: "save_failed",
      status: 500,
    });
    expect(fetch).toHaveBeenCalledOnce();
    expectAliases("Custom name");
    expect(screen.getByTestId("revision")).toHaveTextContent("3");
  });

  it("rejects malformed successful saves without changing aliases or publishing invalidation", async () => {
    const invalidated = vi.fn();
    window.addEventListener(DEVICE_LABELS_CHANGED, invalidated);
    try {
      vi.mocked(fetch).mockResolvedValue(
        jsonResponse({
          revision: 4,
          devices: { [mac]: { label: "Incomplete" } },
        }),
      );
      renderLabels(initial);
      await expect(labels.save(saveInput)).rejects.toMatchObject({
        code: "invalid_response",
      });
      expectAliases("Custom name");
      expect(screen.getByTestId("revision")).toHaveTextContent("3");
      expect(invalidated).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener(DEVICE_LABELS_CHANGED, invalidated);
    }
  });

  it("reports malformed reads without accepting annotation data", async () => {
    vi.mocked(fetch).mockResolvedValue(
      jsonResponse({ revision: 1, devices: { "192.0.2.20": annotation } }),
    );
    renderLabels();
    await waitFor(() =>
      expect(screen.getByTestId("load-error")).toHaveTextContent(
        "invalid_response",
      ),
    );
    expectAliases("System name");
    expect(labels.annotations).toEqual({});
    expect(fetch).toHaveBeenCalledOnce();
  });

  it.each(["be6500panel:unauthorized", "be6500panel:logout"])(
    "clears all names and aborts pending reads and saves on %s",
    async (event) => {
      const storage = storageSpies();
      const { fetch: fetchMock, pending } = pendingFetch();
      const invalidated = vi.fn();
      window.addEventListener(DEVICE_LABELS_CHANGED, invalidated);
      try {
        renderLabels(initial);
        refresh();
        let saveResult!: Promise<unknown>;
        act(() => {
          saveResult = labels.save(saveInput).catch((cause: unknown) => cause);
        });
        await waitFor(() => expect(pending).toHaveLength(2));
        expect(pending.every((request) => !request.signal.aborted)).toBe(true);
        act(() => window.dispatchEvent(new Event(event)));
        await waitFor(() =>
          expect(pending.every((request) => request.signal.aborted)).toBe(true),
        );
        await act(async () => {
          expect(await saveResult).toBeDefined();
        });
        expectAliases("System name");
        expect(labels.annotations).toEqual({});
        expect(screen.getByTestId("revision")).toHaveTextContent("0");
        expect(screen.getByTestId("loading")).toHaveTextContent("false");
        expect(screen.getByTestId("load-error")).toHaveTextContent("none");
        expect(invalidated).toHaveBeenCalledOnce();
        refresh();
        await expect(labels.save(saveInput)).rejects.toMatchObject({
          code: "session_expired",
        });
        expect(fetchMock).toHaveBeenCalledTimes(2);
        for (const spy of storage) expect(spy).not.toHaveBeenCalled();
      } finally {
        window.removeEventListener(DEVICE_LABELS_CHANGED, invalidated);
      }
    },
  );

  it("handles an actual HTTP 401 by clearing loaded names and stopping refreshes", async () => {
    vi.mocked(fetch).mockResolvedValue(
      jsonResponse(
        { error: { code: "unauthorized", message: "Session expired" } },
        401,
      ),
    );
    renderLabels(initial);
    refresh();
    await waitFor(() => expectAliases("System name"));
    expect(labels.annotations).toEqual({});
    expect(screen.getByTestId("revision")).toHaveTextContent("0");
    refresh();
    await expect(labels.save(saveInput)).rejects.toMatchObject({
      code: "session_expired",
    });
    expect(fetch).toHaveBeenCalledOnce();
  });

  it("does not restore private names when a response arrives after logout", async () => {
    let resolveJSON!: (body: unknown) => void;
    const body = new Promise<unknown>((resolve) => {
      resolveJSON = resolve;
    });
    const fetchMock = vi
      .fn()
      .mockResolvedValue({ ok: true, status: 200, json: () => body });
    vi.stubGlobal("fetch", fetchMock);
    renderLabels(initial);
    refresh();
    await waitFor(() => expect(fetchMock).toHaveBeenCalledOnce());
    act(() => window.dispatchEvent(new Event("be6500panel:logout")));
    await act(async () => {
      resolveJSON(changed);
      await body;
    });
    expectAliases("System name");
    expect(labels.annotations).toEqual({});
    expect(screen.getByTestId("revision")).toHaveTextContent("0");
  });

  it("aborts both pending reads and writes when unmounted without publishing their results", async () => {
    const { pending } = pendingFetch();
    const invalidated = vi.fn();
    window.addEventListener(DEVICE_LABELS_CHANGED, invalidated);
    try {
      const { unmount } = renderLabels(initial);
      refresh();
      let saveResult!: Promise<unknown>;
      act(() => {
        saveResult = labels.save(saveInput).catch((cause: unknown) => cause);
      });
      await waitFor(() => expect(pending).toHaveLength(2));
      unmount();
      await waitFor(() =>
        expect(pending.every((request) => request.signal.aborted)).toBe(true),
      );
      expect(await saveResult).toBeDefined();
      expect(invalidated).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener(DEVICE_LABELS_CHANGED, invalidated);
    }
  });
});

describe("actual bounded annotations API response schema", () => {
  it("decodes GET and POST responses including Unicode code-point limits", async () => {
    const bounded: DeviceAnnotations = {
      revision: 0,
      devices: {
        [mac]: {
          label: "😀".repeat(80),
          note: "😀".repeat(1000),
          tags: Array.from({ length: 8 }, () => "😀".repeat(32)),
        },
      },
    };
    vi.mocked(fetch).mockImplementation(() =>
      Promise.resolve(jsonResponse(bounded)),
    );
    await expect(runRequest(deviceAnnotationsAPI.get())).resolves.toEqual(
      bounded,
    );
    await expect(
      runRequest(deviceAnnotationsAPI.save(saveInput)),
    ).resolves.toEqual(bounded);
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it.each([
    { revision: -1, devices: {} },
    { revision: 1.5, devices: {} },
    { revision: Number.MAX_SAFE_INTEGER + 1, devices: {} },
    { devices: {} },
    { revision: 0, devices: { [dashedMAC]: annotation } },
    { revision: 0, devices: { "192.0.2.20": annotation } },
    {
      revision: 0,
      devices: { [mac]: { ...annotation, label: "a".repeat(81) } },
    },
    {
      revision: 0,
      devices: { [mac]: { ...annotation, note: "a".repeat(1001) } },
    },
    {
      revision: 0,
      devices: { [mac]: { ...annotation, tags: ["a".repeat(33)] } },
    },
    {
      revision: 0,
      devices: {
        [mac]: { ...annotation, tags: Array.from({ length: 9 }, () => "tag") },
      },
    },
    { revision: 0, devices: { [mac]: { ...annotation, note: 123 } } },
    { revision: 0, devices: { [mac]: { label: "Missing metadata" } } },
    {
      revision: 0,
      devices: Object.fromEntries(
        Array.from({ length: 257 }, (_, index) => [
          `02:00:00:00:${Math.floor(index / 256)
            .toString(16)
            .padStart(
              2,
              "0",
            )}:${(index % 256).toString(16).padStart(2, "0")}`.toUpperCase(),
          annotation,
        ]),
      ),
    },
  ])("rejects malformed or unbounded response %#", async (payload) => {
    vi.mocked(fetch).mockImplementation(() =>
      Promise.resolve(jsonResponse(payload)),
    );
    await expect(runRequest(deviceAnnotationsAPI.get())).rejects.toMatchObject({
      code: "invalid_response",
    });
    await expect(
      runRequest(deviceAnnotationsAPI.save(saveInput)),
    ).rejects.toMatchObject({ code: "invalid_response" });
    expect(fetch).toHaveBeenCalledTimes(2);
  });
});
