import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, runRequest } from "../../lib/api";
import { configurationApi } from "./client";
import type {
  ConfigurationOperationStatus,
  ConfigurationStatus,
} from "./contracts";

export const recoveryPollIntervalMs = 10_000;
export type RecoveryAction = "confirm" | "rollback";
interface RecoveryState {
  status?: ConfigurationStatus;
  lastUnresolved?: ConfigurationOperationStatus;
  outcome?: ConfigurationOperationStatus;
  loading: boolean;
  refreshing: boolean;
  stale: boolean;
  busy?: RecoveryAction;
  error?: unknown;
}
const initialState: RecoveryState = {
  loading: true,
  refreshing: false,
  stale: false,
};
const unresolved = (operation?: ConfigurationOperationStatus) =>
  !!operation &&
  ["applying", "pending", "rolling_back"].includes(operation.phase);
const phaseOrder = {
  applying: 0,
  pending: 1,
  committed: 2,
  rolling_back: 3,
  rolled_back: 4,
};
const observedOperation = (
  status: ConfigurationStatus,
): ConfigurationOperationStatus | undefined =>
  status.operation ??
  (status.pendingCommit
    ? {
        id: status.pendingCommit.id,
        deadline: status.pendingCommit.deadline,
        generation: status.generation,
        changedModules: [],
        state: "pending_confirmation",
        phase: "pending",
        canConfirm: status.enabled,
        canRollback: true,
      }
    : undefined);

/** No documents or drafts are loaded here. Only explicit clicks make mutations. */
export function useGlobalRecoveryState() {
  const [state, setState] = useState<RecoveryState>(initialState);
  const current = useRef(state);
  const lifetime = useRef<AbortController | undefined>(undefined);
  const readController = useRef<AbortController | undefined>(undefined);
  const readSequence = useRef(0);
  const actionInFlight = useRef(false);
  const update = useCallback(
    (change: (previous: RecoveryState) => RecoveryState) => {
      current.current = change(current.current);
      setState(current.current);
    },
    [],
  );
  const active = useCallback(
    (controller: AbortController) =>
      lifetime.current === controller && !controller.signal.aborted,
    [],
  );

  const clear = useCallback(() => {
    lifetime.current?.abort();
    readController.current?.abort();
    readSequence.current++;
    actionInFlight.current = false;
    update(() => ({ ...initialState, loading: false }));
  }, [update]);

  const refreshStatus = useCallback(
    async (afterAction = false) => {
      const owner = lifetime.current;
      if (!owner || !active(owner) || (actionInFlight.current && !afterAction))
        return;
      // Cancel and supersede any earlier read. Its result cannot replace a newer ID,
      // terminal phase, or post-mutation observation even if fetch ignores abort.
      readController.current?.abort();
      const controller = new AbortController();
      readController.current = controller;
      const sequence = ++readSequence.current;
      const valid = () =>
        active(owner) &&
        !controller.signal.aborted &&
        sequence === readSequence.current;
      update((previous) => ({ ...previous, refreshing: true }));
      try {
        const status = await runRequest(
          configurationApi.status(),
          AbortSignal.any([owner.signal, controller.signal]),
        );
        if (!valid()) return;
        const oldStatus = current.current.status;
        const oldOperation = oldStatus
          ? observedOperation(oldStatus)
          : undefined;
        const nextOperation = observedOperation(status);
        if (
          oldStatus &&
          (status.generation < oldStatus.generation ||
            (oldOperation &&
              nextOperation &&
              (nextOperation.generation < oldOperation.generation ||
                (nextOperation.id === oldOperation.id &&
                  phaseOrder[nextOperation.phase] <
                    phaseOrder[oldOperation.phase]))))
        )
          throw new ApiError({
            code: "stale_response",
            message: "读取到较早的配置状态，请重新核对",
          });
        update((previous) => {
          const operation = nextOperation;
          const previousUnresolved = previous.lastUnresolved;
          const outcome =
            operation &&
            !unresolved(operation) &&
            operation.id === previousUnresolved?.id
              ? operation
              : previous.outcome?.id === operation?.id
                ? previous.outcome
                : undefined;
          return {
            ...previous,
            status,
            lastUnresolved: unresolved(operation)
              ? operation
              : !operation
                ? previousUnresolved
                : undefined,
            outcome,
            loading: false,
            stale: false,
            error: afterAction ? previous.error : undefined,
          };
        });
      } catch (error) {
        if (valid())
          update((previous) => ({
            ...previous,
            loading: false,
            stale: true,
            error: previous.busy ? (previous.error ?? error) : error,
          }));
      } finally {
        if (valid()) update((previous) => ({ ...previous, refreshing: false }));
      }
    },
    [active, update],
  );

  const refresh = useCallback(() => refreshStatus(), [refreshStatus]);
  useEffect(() => {
    const controller = new AbortController();
    lifetime.current = controller;
    void refresh();
    const interval = window.setInterval(() => {
      void refresh();
    }, recoveryPollIntervalMs);
    const reconnect = () => {
      void refresh();
    };
    const visible = () => {
      if (document.visibilityState === "visible") void refresh();
    };
    window.addEventListener("online", reconnect);
    window.addEventListener("focus", reconnect);
    document.addEventListener("visibilitychange", visible);
    window.addEventListener("be6500panel:unauthorized", clear);
    return () => {
      controller.abort();
      readController.current?.abort();
      readSequence.current++;
      if (lifetime.current === controller) lifetime.current = undefined;
      window.clearInterval(interval);
      window.removeEventListener("online", reconnect);
      window.removeEventListener("focus", reconnect);
      document.removeEventListener("visibilitychange", visible);
      window.removeEventListener("be6500panel:unauthorized", clear);
    };
  }, [clear, refresh]);

  const perform = useCallback(
    async (action: RecoveryAction) => {
      const owner = lifetime.current;
      const previous = current.current;
      const status = previous.status;
      if (
        !owner ||
        !active(owner) ||
        actionInFlight.current ||
        previous.stale ||
        !status
      )
        return;
      const operation = status.operation;
      const pending = status.pendingCommit;
      const id = operation?.id ?? pending?.id;
      const deadline = operation?.deadline ?? pending?.deadline;
      const canConfirm = operation
        ? operation.canConfirm && operation.phase === "pending"
        : !!pending && status.enabled;
      const canRollback = operation
        ? operation.canRollback && unresolved(operation)
        : !!pending;
      if (
        !id ||
        (action === "confirm"
          ? !canConfirm ||
            !deadline ||
            Date.parse(deadline) <= Date.now() ||
            !Number.isFinite(Date.parse(deadline))
          : !canRollback)
      )
        return;
      actionInFlight.current = true;
      readController.current?.abort();
      readSequence.current++;
      update((old) => ({
        ...old,
        busy: action,
        refreshing: false,
        error: undefined,
      }));
      try {
        // Never retry/replay POST, and never infer recovery from its missing response.
        await runRequest(
          action === "confirm"
            ? configurationApi.confirm(id)
            : configurationApi.rollback(id),
          owner.signal,
        );
      } catch (error) {
        if (active(owner)) update((old) => ({ ...old, error }));
      } finally {
        if (active(owner)) {
          await refreshStatus(true);
          if (active(owner)) update((old) => ({ ...old, busy: undefined }));
        }
        actionInFlight.current = false;
      }
    },
    [active, refreshStatus, update],
  );
  const confirm = useCallback(() => perform("confirm"), [perform]);
  const rollback = useCallback(() => perform("rollback"), [perform]);
  const dismissOutcome = useCallback(
    () => update((previous) => ({ ...previous, outcome: undefined })),
    [update],
  );
  return { ...state, refresh, confirm, rollback, clear, dismissOutcome };
}
export type GlobalRecoveryController = ReturnType<
  typeof useGlobalRecoveryState
>;
