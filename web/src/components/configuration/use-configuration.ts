import { useCallback, useEffect, useRef, useState } from "react";
import type { Effect } from "effect";
import { ApiError, runRequest } from "../../lib/api";
import { configurationApi } from "./client";
import type {
  ConfigurationCommit,
  ConfigurationDraft,
  ConfigurationModule,
  ConfigurationSnapshot,
  ConfigurationStatus,
} from "./contracts";

export interface EditorBuffer {
  content: string;
  savedContent: string;
  generation: number;
  stagedContent?: string;
  stagedId?: string;
}
interface ConfigurationState {
  status?: ConfigurationStatus;
  snapshot?: ConfigurationSnapshot;
  drafts: readonly ConfigurationDraft[];
  buffers: Partial<Record<ConfigurationModule, EditorBuffer>>;
  selectedIds: readonly string[];
  loading: boolean;
  busy?: string;
  error?: unknown;
  operation?: ConfigurationCommit;
}
const initialState: ConfigurationState = {
  drafts: [],
  buffers: {},
  selectedIds: [],
  loading: true,
};
export const isDirty = (buffer: EditorBuffer) =>
  buffer.content !== buffer.savedContent;

/** Each page owns its requests and timers. Private documents stay in memory. */
export function useConfiguration() {
  const [state, setState] = useState<ConfigurationState>(initialState);
  const current = useRef(state);
  const lifetime = useRef<AbortController | undefined>(undefined);
  const update = useCallback(
    (change: (state: ConfigurationState) => ConfigurationState) => {
      current.current = change(current.current);
      setState(current.current);
    },
    [],
  );
  const run = useCallback(
    <A>(
      effect: Effect.Effect<A, ApiError>,
      signal = lifetime.current?.signal,
    ) => runRequest(effect, signal),
    [],
  );
  const active = useCallback(
    (signal = lifetime.current?.signal) =>
      !!signal && signal === lifetime.current?.signal && !signal.aborted,
    [],
  );

  const readAll = useCallback(async () => {
    const signal = lifetime.current?.signal;
    const status = await run(configurationApi.status(), signal);
    if (!active(signal)) return;
    update((previous) => ({ ...previous, status }));
    if (!status.enabled) {
      update((previous) => ({
        ...previous,
        status,
        snapshot: undefined,
        buffers: {},
        drafts: [],
        selectedIds: [],
      }));
      return;
    }
    const [snapshot, response] = await Promise.all([
      run(configurationApi.read(), signal),
      run(configurationApi.drafts(), signal),
    ]);
    if (!active(signal)) return;
    update((previous) => {
      const buffers: ConfigurationState["buffers"] = {};
      snapshot.documents.forEach((document) => {
        const old = previous.buffers[document.module];
        buffers[document.module] =
          old && isDirty(old)
            ? {
                ...old,
                generation:
                  old.savedContent === document.content
                    ? snapshot.generation
                    : old.generation,
              }
            : {
                content: document.content,
                savedContent: document.content,
                generation: snapshot.generation,
              };
      });
      const eligible = response.drafts.filter(
        (draft) => draft.valid && draft.generation === snapshot.generation,
      );
      // Keep selection intentional after first load; initially select the newest
      // eligible draft per module, not competing versions of the same document.
      const selectedIds = previous.snapshot
        ? previous.selectedIds.filter((id) =>
            eligible.some((draft) => draft.id === id),
          )
        : [
            ...new Map(eligible.map((draft) => [draft.module, draft])).values(),
          ].map((draft) => draft.id);
      return {
        ...previous,
        status: {
          ...status,
          generation: snapshot.generation,
          pendingCommit: snapshot.pendingCommit,
        },
        snapshot,
        drafts: response.drafts,
        buffers,
        selectedIds,
      };
    });
  }, [active, run, update]);

  const refresh = useCallback(async () => {
    if (current.current.busy) return;
    const signal = lifetime.current?.signal;
    update((previous) => ({ ...previous, busy: "refresh", error: undefined }));
    try {
      await readAll();
    } catch (error) {
      if (active(signal)) update((previous) => ({ ...previous, error }));
    } finally {
      if (active(signal))
        update((previous) => ({
          ...previous,
          loading: false,
          busy: undefined,
        }));
    }
  }, [active, readAll, update]);

  const reconcile = useCallback(async () => {
    if (current.current.busy) return;
    const signal = lifetime.current?.signal;
    update((previous) => ({ ...previous, busy: "status" }));
    try {
      const status = await run(configurationApi.status(), signal);
      if (!active(signal)) return;
      const previous = current.current;
      update((old) => ({ ...old, status, error: undefined }));
      if (
        (previous.status?.pendingCommit && !status.pendingCommit) ||
        status.generation !== previous.snapshot?.generation
      )
        await readAll();
    } catch (error) {
      if (active(signal)) update((previous) => ({ ...previous, error }));
    } finally {
      if (active(signal))
        update((previous) => ({ ...previous, busy: undefined }));
    }
  }, [active, readAll, run, update]);

  useEffect(() => {
    const controller = new AbortController();
    lifetime.current = controller;
    void refresh();
    return () => {
      controller.abort();
      if (lifetime.current === controller) {
        lifetime.current = undefined;
        current.current = { ...current.current, busy: undefined };
      }
    };
  }, [refresh]);

  const pendingId = state.status?.pendingCommit?.id;
  useEffect(() => {
    if (!pendingId) return;
    const interval = window.setInterval(() => {
      void reconcile();
    }, 10_000);
    return () => window.clearInterval(interval);
  }, [pendingId, reconcile]);

  const mutate = useCallback(
    async <A>(
      name: string,
      effect: Effect.Effect<A, ApiError>,
      accept: (value: A) => void | Promise<void>,
    ) => {
      if (current.current.busy || !active()) return;
      const signal = lifetime.current?.signal;
      update((previous) => ({ ...previous, busy: name, error: undefined }));
      try {
        const result = await run(effect, signal);
        if (active(signal)) await accept(result);
      } catch (error) {
        if (active(signal)) {
          update((previous) => ({ ...previous, error }));
          // Failed or timed-out writes can still have changed router state.
          // Reconcile without replaying the write or discarding local text.
          try {
            const status = await run(configurationApi.status(), signal);
            if (active(signal)) {
              update((previous) => ({ ...previous, status }));
              if (status.generation !== current.current.snapshot?.generation)
                await readAll();
            }
          } catch {
            /* Keep the original mutation failure visible. */
          }
        }
      } finally {
        if (active(signal))
          update((previous) => ({ ...previous, busy: undefined }));
      }
    },
    [active, readAll, run, update],
  );

  const edit = (module: ConfigurationModule, content: string) =>
    update((previous) => {
      const buffer = previous.buffers[module];
      return buffer
        ? {
            ...previous,
            buffers: { ...previous.buffers, [module]: { ...buffer, content } },
          }
        : previous;
    });
  const reset = (module: ConfigurationModule) =>
    update((previous) => {
      const document = previous.snapshot?.documents.find(
        (document) => document.module === module,
      );
      if (!document || !previous.snapshot) return previous;
      return {
        ...previous,
        buffers: {
          ...previous.buffers,
          [module]: {
            content: document.content,
            savedContent: document.content,
            generation: previous.snapshot.generation,
          },
        },
      };
    });
  const select = (draft: ConfigurationDraft, checked: boolean) =>
    update((previous) => ({
      ...previous,
      selectedIds: checked
        ? [
            ...previous.selectedIds.filter(
              (id) =>
                previous.drafts.find((item) => item.id === id)?.module !==
                draft.module,
            ),
            draft.id,
          ]
        : previous.selectedIds.filter((id) => id !== draft.id),
    }));
  const stage = (module: ConfigurationModule) => {
    const buffer = current.current.buffers[module];
    const status = current.current.status;
    if (
      !buffer ||
      !status?.enabled ||
      status.pendingCommit ||
      buffer.generation !== status.generation
    )
      return;
    const content = buffer.content;
    return mutate(
      "stage",
      configurationApi.stage({
        module,
        content,
        generation: buffer.generation,
      }),
      (draft) => {
        update((previous) => ({
          ...previous,
          drafts: [
            ...previous.drafts.filter((item) => item.id !== draft.id),
            draft,
          ],
          selectedIds: [
            ...previous.selectedIds.filter(
              (id) =>
                previous.drafts.find((item) => item.id === id)?.module !==
                module,
            ),
            ...(draft.valid ? [draft.id] : []),
          ],
          buffers: {
            ...previous.buffers,
            [module]: {
              ...previous.buffers[module]!,
              stagedContent: content,
              stagedId: draft.id,
            },
          },
        }));
      },
    );
  };
  const remove = (id: string) =>
    mutate("delete", configurationApi.remove(id), () => {
      update((previous) => ({
        ...previous,
        drafts: previous.drafts.filter((draft) => draft.id !== id),
        selectedIds: previous.selectedIds.filter((selected) => selected !== id),
        buffers: Object.fromEntries(
          Object.entries(previous.buffers).map(([module, buffer]) => [
            module,
            buffer.stagedId === id
              ? { ...buffer, stagedId: undefined, stagedContent: undefined }
              : buffer,
          ]),
        ),
      }));
    });
  const acceptOperation = useCallback(
    async (operation: ConfigurationCommit) => {
      update((previous) => {
        const pendingCommit =
          operation.state === "pending_confirmation" && operation.deadline
            ? { id: operation.id, deadline: operation.deadline }
            : undefined;
        const buffers = { ...previous.buffers };
        if (operation.state !== "rolled_back")
          operation.changedModules.forEach((module) => {
            const buffer = buffers[module];
            // Clear only text included in this Commit; preserve subsequent edits.
            if (
              buffer?.stagedId &&
              previous.selectedIds.includes(buffer.stagedId) &&
              buffer.content === buffer.stagedContent
            ) {
              buffers[module] = {
                ...buffer,
                savedContent: buffer.content,
                generation: operation.generation,
                stagedContent: undefined,
                stagedId: undefined,
              };
            }
          });
        return {
          ...previous,
          buffers,
          operation,
          status: {
            enabled: previous.status?.enabled ?? true,
            generation: operation.generation,
            pendingCommit,
          },
        };
      });
      try {
        await readAll();
      } catch (error) {
        if (active()) update((previous) => ({ ...previous, error }));
      }
    },
    [active, readAll, update],
  );

  const commit = (
    acknowledgeRisks: boolean,
    ids = current.current.selectedIds,
  ) => {
    const { drafts, status } = current.current;
    const selected = drafts.filter((draft) => ids.includes(draft.id));
    if (
      !status?.enabled ||
      status.pendingCommit ||
      !selected.length ||
      selected.length !== ids.length ||
      selected.some(
        (draft) => !draft.valid || draft.generation !== status.generation,
      )
    )
      return;
    return mutate(
      "commit",
      configurationApi.commit({
        draftIds: ids,
        generation: status.generation,
        acknowledgeRisks,
      }),
      (operation) => acceptOperation(operation),
    );
  };
  const confirm = () => {
    const pending = current.current.status?.pendingCommit;
    if (
      !pending ||
      !Number.isFinite(Date.parse(pending.deadline)) ||
      Date.parse(pending.deadline) <= Date.now()
    )
      return;
    return mutate(
      "confirm",
      configurationApi.confirm(pending.id),
      (operation) => acceptOperation(operation),
    );
  };
  const rollback = () => {
    const pending = current.current.status?.pendingCommit;
    if (!pending) return;
    return mutate(
      "rollback",
      configurationApi.rollback(pending.id),
      (operation) => acceptOperation(operation),
    );
  };
  return {
    ...state,
    edit,
    reset,
    select,
    stage,
    remove,
    commit,
    confirm,
    rollback,
    refresh,
    reconcile,
  };
}
export type ConfigurationController = ReturnType<typeof useConfiguration>;
