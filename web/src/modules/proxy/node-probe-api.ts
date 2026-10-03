import { Effect, Schema } from "effect";
import { ApiError, request } from "../../lib/api";
import {
  NodeProbeRunInputSchema,
  NodeProbeSnapshotSchema,
  type NodeProbeRunInput,
} from "./node-probe-contracts";

/** The service owns jobs. Reads never start a job; only explicit POST/DELETE mutate it. */
export const nodeProbeApi = {
  snapshot: () => request("/proxy/node-probes", NodeProbeSnapshotSchema),
  start: (input: NodeProbeRunInput) =>
    Schema.decodeUnknown(NodeProbeRunInputSchema)(input).pipe(
      Effect.mapError(
        () =>
          new ApiError({
            code: "invalid_request",
            message: "请选择 1–256 个不同节点，或以空节点列表测速全部节点",
          }),
      ),
      Effect.flatMap(({ all, nodeIds, revision }) =>
        request("/proxy/node-probes", NodeProbeSnapshotSchema, {
          method: "POST",
          body: { all, nodeIds, revision },
          // The service bounds lease admission to10 seconds, not the full job.
          timeoutMs: 12000,
        }),
      ),
    ),
  stop: () =>
    request("/proxy/node-probes", NodeProbeSnapshotSchema, {
      method: "DELETE",
    }),
};
