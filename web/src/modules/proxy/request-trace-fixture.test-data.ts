import type {
  RequestTrace,
  RequestTraceHistory,
} from "./request-trace-contracts";

export const requestTraceFixture: RequestTrace = {
  id: "trace-actual",
  targetId: "google204",
  targetLabel: "Google 204",
  url: "https://www.gstatic.com/generate_204",
  route: "proxy",
  startedAt: "2026-10-03T00:00:00Z",
  finishedAt: "2026-10-03T00:00:00.090Z",
  totalMs: 90,
  outcome: "failed",
  statusCode: null,
  bytesRead: 0,
  bodyLimitReached: false,
  peerAddress: "127.0.0.1:7890",
  peerScope: "proxy",
  failurePhase: "tls",
  errorCode: "tls_failed",
  phases: [
    {
      id: "dns",
      observed: false,
      startMs: null,
      endMs: null,
      durationMs: null,
      reason: "remote DNS is not observed",
    },
    { id: "tcp", observed: true, startMs: 10, endMs: 35, durationMs: 25 },
    { id: "connect", observed: true, startMs: 30, endMs: 50, durationMs: 20 },
    {
      id: "tls",
      observed: true,
      startMs: 50,
      endMs: null,
      durationMs: null,
      reason: "origin TLS handshake failed",
    },
    {
      id: "ttfb",
      observed: false,
      startMs: null,
      endMs: null,
      durationMs: null,
    },
    {
      id: "transfer",
      observed: false,
      startMs: null,
      endMs: null,
      durationMs: null,
    },
  ],
};
export const requestTraceHistoryFixture: RequestTraceHistory = {
  traces: [requestTraceFixture],
  targets: [
    {
      id: "google204",
      label: "Google 204",
      url: "https://www.gstatic.com/generate_204",
    },
    {
      id: "cloudflare",
      label: "Cloudflare",
      url: "https://www.cloudflare.com/cdn-cgi/trace",
    },
  ],
  limits: { timeoutMs: 10000, bodyBytes: 65536, concurrency: 1, capacity: 64 },
  running: false,
};
