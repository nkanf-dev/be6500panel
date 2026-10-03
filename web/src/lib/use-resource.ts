import { useCallback, useEffect, useState } from "react";
import type { Effect } from "effect";
import { runRequest, type ApiError } from "./api";

interface ResourceState<A> {
  load: () => Effect.Effect<A, ApiError>;
  data?: A;
  error?: unknown;
  loading: boolean;
}
export function useResource<A>(load: () => Effect.Effect<A, ApiError>) {
  const [state, setState] = useState<ResourceState<A>>({ load, loading: true });
  const [revision, setRevision] = useState(0);
  const reload = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    let active = true;
    const controller = new AbortController();
    setState((previous) =>
      previous.load === load ? previous : { load, loading: true },
    );
    runRequest(load(), controller.signal)
      .then((data) => {
        if (active) setState({ load, data, loading: false });
      })
      .catch((error) => {
        if (active)
          setState((previous) => ({ ...previous, error, loading: false }));
      });
    return () => {
      active = false;
      controller.abort();
    };
  }, [load, revision]);
  const current: ResourceState<A> =
    state.load === load ? state : { load, loading: true };
  return {
    data: current.data,
    error: current.error,
    loading: current.loading,
    reload,
  };
}
