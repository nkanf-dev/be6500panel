import { useCallback, useEffect, useState } from "react";
import type { Effect } from "effect";
import { runRequest, type ApiError } from "./api";
export function useResource<A>(load: () => Effect.Effect<A, ApiError>) {
  const [data, setData] = useState<A>();
  const [error, setError] = useState<unknown>();
  const [loading, setLoading] = useState(true);
  const [revision, setRevision] = useState(0);
  const reload = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    let active = true;
    const controller = new AbortController();
    setLoading(true);
    runRequest(load(), controller.signal)
      .then((value) => {
        if (active) {
          setData(value);
          setError(undefined);
        }
      })
      .catch((error) => {
        if (active) setError(error);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
      controller.abort();
    };
  }, [load, revision]);
  return { data, error, loading, reload };
}
