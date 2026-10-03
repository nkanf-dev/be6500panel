import { useCallback, useEffect, useRef, useState } from "react";
import { runRequest } from "../../lib/api";
import { nodeProbeApi } from "./node-probe-api";
import {
  NODE_PROBE_POLL_MS,
  type NodeProbeRunInput,
  type NodeProbeSnapshot,
} from "./node-probe-contracts";

type ProbeSelection = Pick<NodeProbeRunInput, "all" | "nodeIds">;
type PendingAction = "start" | "stop";

/** Observe service-owned jobs. Mount, refresh and revision changes never POST. */
export function useNodeProbes(revision?: string) {
  const [snapshot, setSnapshot] = useState<NodeProbeSnapshot>();
  const [readError, setReadError] = useState<unknown>();
  const [actionError, setActionError] = useState<unknown>();
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState<PendingAction>();
  const [starting, setStarting] = useState<ProbeSelection>();
  const [refreshVersion, setRefreshVersion] = useState(0);
  const mounted = useRef(false);
  const mutation = useRef<PendingAction | undefined>(undefined);
  const latest = useRef<NodeProbeSnapshot | undefined>(undefined);
  const readEpoch = useRef(0);
  const refresh = useCallback(
    () => setRefreshVersion((value) => value + 1),
    [],
  );

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    // Pause passive reads around a mutation so an older GET cannot overwrite
    // the snapshot returned by an accepted POST or DELETE.
    if (pending !== undefined) {
      setLoading(false);
      return;
    }
    const controller = new AbortController();
    const epoch = ++readEpoch.current;
    let current = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const load = async () => {
      setLoading(!latest.current || (revision !== undefined && latest.current.revision !== revision));
      try {
        const value = await runRequest(
          nodeProbeApi.snapshot(),
          controller.signal,
        );
        if (
          current &&
          epoch === readEpoch.current &&
          mutation.current === undefined
        ) {
          latest.current = value;
          setSnapshot(value);
          setReadError(undefined);
        }
      } catch (cause) {
        if (
          current &&
          epoch === readEpoch.current &&
          mutation.current === undefined
        )
          setReadError(cause);
      } finally {
        if (current && epoch === readEpoch.current) {
          setLoading(false);
          // Keep observing an active job even if one passive read fails.
          if (mutation.current === undefined && latest.current?.running)
            timer = setTimeout(() => void load(), NODE_PROBE_POLL_MS);
        }
      }
    };
    void load();
    return () => {
      current = false;
      controller.abort();
      clearTimeout(timer);
    };
  }, [revision, refreshVersion, pending]);

  const mutate = useCallback(
    async (action: PendingAction, input?: ProbeSelection) => {
      const value = latest.current;
      if (mutation.current !== undefined || !value) return;
      const stale = revision !== undefined && value.revision !== revision;
      if (
        action === "start" &&
        (!input || !value.available || value.running || stale)
      )
        return;
      if (action === "stop" && !value.running) return;
      mutation.current = action;
      readEpoch.current += 1;
      setPending(action);
      setActionError(undefined);
      if (action === "start") setStarting(input);
      try {
        // Do not attach the component's AbortSignal: leaving the page only
        // cancels GET polling. The server job stops solely via explicit DELETE.
        const next = await runRequest(
          action === "start"
            ? nodeProbeApi.start({
                ...input!,
                revision: revision ?? value.revision,
              })
            : nodeProbeApi.stop(),
        );
        if (mounted.current) {
          latest.current = next;
          setSnapshot(next);
          setReadError(undefined);
        }
      } catch (cause) {
        if (mounted.current) setActionError(cause);
      } finally {
        mutation.current = undefined;
        if (mounted.current) {
          setPending(undefined);
          setStarting(undefined);
          refresh();
        }
      }
    },
    [revision, refresh],
  );
  const start = useCallback(
    (input: ProbeSelection) => mutate("start", input),
    [mutate],
  );
  const stop = useCallback(() => mutate("stop"), [mutate]);
  const stale =
    snapshot !== undefined &&
    revision !== undefined &&
    snapshot.revision !== revision;
  const running = pending === "start" || snapshot?.running === true;
  return {
    snapshot,
    results: stale ? [] : (snapshot?.results ?? []),
    stale,
    error: actionError ?? readError,
    loading,
    pending,
    starting,
    running,
    canStart:
      !!snapshot?.available && !stale && !running && !pending && !loading,
    progress:
      pending === "start"
        ? {
            completed: 0,
            total: starting?.all ? 0 : (starting?.nodeIds.length ?? 0),
          }
        : {
            completed: snapshot?.job?.completed ?? 0,
            total: snapshot?.job?.total ?? 0,
          },
    refresh,
    start,
    stop,
  };
}

export type NodeProbeController = ReturnType<typeof useNodeProbes>;
