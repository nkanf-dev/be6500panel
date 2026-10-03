import { useId, useMemo, useState } from "react";
import type { EChartsOption, LineSeriesOption } from "echarts";
import { strings } from "../../locales/strings";
import { ChartFrame, ChartSelect } from "./ChartFrame";
import { EChart } from "./EChart";
import {
  axisStyle,
  baseOption,
  zoomOption,
  type ChartPalette,
} from "./chart-theme";
import { phaseLabels, phases, requestSamples } from "./demo-data";
import {
  REQUEST_TRACE_PHASES,
  type RequestTrace,
  type RequestTracePhase,
  type RequestTracePhaseId,
} from "../../modules/proxy/request-trace-contracts";
import { requestTraceCopy as copy } from "../../modules/proxy/request-trace-copy";

export interface RequestWaterfallProps {
  demo?: boolean;
  traces?: readonly RequestTrace[];
}
const phaseColor = (phase: RequestTracePhaseId, palette: ChartPalette) => {
  const colors = {
    dns: palette.dns,
    tcp: palette.connect,
    connect: palette.latency,
    tls: palette.tls,
    ttfb: palette.wait,
    transfer: palette.transfer,
  };
  return colors[phase];
};
const phaseName = (phase: RequestTracePhase, trace: RequestTrace) => {
  if (phase.id === "tls") return copy.originTLS;
  if (phase.id === "tcp") {
    if (phase.reason === "multiple_connection_attempt_span")
      return trace.route === "proxy" ? copy.multipleProxyTCP : copy.multipleTCP;
    if (trace.route === "proxy") return copy.proxyTCP;
  }
  return copy.phases[phase.id];
};
const phaseReason = (phase: RequestTracePhase) =>
  phase.reason
    ? (copy.reasons[phase.reason as keyof typeof copy.reasons] ?? phase.reason)
    : phase.observed
      ? "—"
      : copy.notObservedReason;
const offset = (value: number | null) =>
  value === null ? copy.unknown : String(value);
const observation = (phase: RequestTracePhase) =>
  !phase.observed
    ? copy.notObserved
    : phase.startMs !== null && phase.endMs === null
      ? copy.incomplete
      : copy.observed;
const routeLabel = (route: RequestTrace["route"]) =>
  route === "direct" ? copy.modeDirect : copy.modeProxy;
const terminalLabel = (trace: RequestTrace) =>
  [trace.failurePhase, trace.errorCode].filter(Boolean).join(" · ");

/** Independent segments use observed offsets. Their durations are never stacked or added. */
export function requestWaterfallOption(
  traces: readonly RequestTrace[],
  palette: ChartPalette,
): EChartsOption {
  const series: LineSeriesOption[] = [];
  for (const [row, trace] of traces.entries()) {
    for (const phase of trace.phases) {
      if (!phase.observed || phase.startMs === null) continue;
      const color = phaseColor(phase.id, palette);
      // Small parallel lanes keep overlaps visible within each request row.
      const lane = row - 0.15 + REQUEST_TRACE_PHASES.indexOf(phase.id) * 0.06;
      series.push({
        type: "line",
        name: phaseName(phase, trace),
        data:
          phase.endMs === null
            ? [[phase.startMs, lane]]
            : [
                [phase.startMs, lane],
                [phase.endMs, lane],
              ],
        symbol: phase.endMs === null ? "emptyCircle" : "circle",
        symbolSize: phase.endMs === null ? 8 : 4,
        showSymbol: true,
        lineStyle: { width: 5, color },
        itemStyle: { color },
        tooltip: {
          formatter: () =>
            `${trace.targetLabel} · ${routeLabel(trace.route)} · ${copy.outcomes[trace.outcome]}\n${phaseName(phase, trace)} · ${observation(phase)}\n${copy.columns.start}: ${offset(phase.startMs)}\n${copy.columns.end}: ${offset(phase.endMs)}\n${copy.columns.duration}: ${offset(phase.durationMs)}\n${phaseReason(phase)}`,
        },
      });
    }
    if (trace.outcome !== "success")
      series.push({
        type: "line",
        name: copy.failureMarker,
        data: [[trace.totalMs, row]],
        symbol: "diamond",
        symbolSize: 10,
        showSymbol: true,
        lineStyle: { width: 0 },
        itemStyle: { color: palette.blocked },
        tooltip: {
          formatter: () =>
            `${trace.targetLabel} · ${copy.outcomes[trace.outcome]}\n${trace.totalMs} ms · ${terminalLabel(trace) || copy.notProvided}`,
        },
      });
  }
  return {
    ...baseOption(palette),
    legend: {
      ...(baseOption(palette).legend as object),
      data: [...new Set(series.map((item) => item.name as string))],
      selectedMode: false,
    },
    grid: { top: 62, right: 28, bottom: 60, left: 12, containLabel: true },
    xAxis: {
      type: "value",
      name: "ms",
      min: 0,
      max: Math.max(
        1,
        ...traces.flatMap((trace) => [
          trace.totalMs,
          ...trace.phases.flatMap((phase) => [
            phase.startMs ?? 0,
            phase.endMs ?? 0,
          ]),
        ]),
      ),
      ...axisStyle(palette),
    },
    yAxis: {
      type: "value",
      inverse: true,
      min: -1,
      max: Math.max(1, traces.length),
      interval: 1,
      ...axisStyle(palette),
      axisLabel: {
        color: palette.text,
        fontSize: 10,
        width: 160,
        overflow: "truncate",
        formatter: (value: number) =>
          traces[value]
            ? `${traces[value].id} · ${traces[value].targetLabel} · ${routeLabel(traces[value].route)}`
            : "",
      },
      splitLine: { show: false },
    },
    tooltip: { ...baseOption(palette).tooltip, trigger: "item" },
    dataZoom: zoomOption(palette),
    series,
  };
}

export function RequestWaterfall({
  demo = false,
  traces = [],
}: RequestWaterfallProps) {
  return demo ? (
    <DemoRequestWaterfall />
  ) : (
    <ActualRequestWaterfall traces={traces} />
  );
}
function ActualRequestWaterfall({
  traces,
}: {
  traces: readonly RequestTrace[];
}) {
  const id = useId();
  const [route, setRoute] = useState("all");
  const [outcome, setOutcome] = useState("all");
  const filtered = useMemo(
    () =>
      traces.filter(
        (trace) =>
          (route === "all" || trace.route === route) &&
          (outcome === "all" || trace.outcome === outcome),
      ),
    [traces, route, outcome],
  );
  const option = useMemo(
    () => (palette: ChartPalette) => requestWaterfallOption(filtered, palette),
    [filtered],
  );
  const columns = [
    copy.columns.trace,
    copy.columns.route,
    copy.columns.outcome,
    copy.columns.phase,
    copy.columns.observation,
    copy.columns.start,
    copy.columns.end,
    copy.columns.duration,
    copy.columns.total,
    copy.columns.reason,
    copy.columns.peer,
    copy.columns.status,
    copy.columns.bytes,
    copy.columns.bodyLimit,
  ];
  return (
    <section className="viz-panel" aria-labelledby={`${id}-title`}>
      <header className="viz-header">
        <div>
          <h3 id={`${id}-title`}>{copy.title}</h3>
          <p className="viz-subtitle">{copy.subtitle}</p>
        </div>
        <span className="viz-source">{copy.source}</span>
      </header>
      <p className="viz-summary">{copy.badgeDiagnostic}</p>
      {traces.length > 0 && (
        <div className="viz-controls">
          <ChartSelect
            label={copy.routeFilter}
            value={route}
            onChange={setRoute}
          >
            <option value="all">{copy.all}</option>
            <option value="direct">{copy.modeDirect}</option>
            <option value="proxy">{copy.modeProxy}</option>
          </ChartSelect>
          <ChartSelect
            label={copy.outcomeFilter}
            value={outcome}
            onChange={setOutcome}
          >
            <option value="all">{copy.all}</option>
            {Object.entries(copy.outcomes).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </ChartSelect>
        </div>
      )}
      {filtered.length > 0 ? (
        <>
          <p className="viz-summary">
            {filtered.length} {copy.records}
          </p>
          <EChart
            option={option}
            label={copy.chartLabel}
            height={Math.min(480, Math.max(240, filtered.length * 44 + 120))}
          />
          <p className="viz-summary">{copy.offsetHint}</p>
          <p className="viz-summary">{copy.phaseScope}</p>
          <footer className="viz-footer">
            <span>{copy.source}</span>
            <span>{copy.limits}</span>
          </footer>
          <details className="viz-table-details">
            <summary>{copy.phaseDetail}</summary>
            <div className="viz-table-scroll">
              <table>
                <caption>
                  {copy.title} · {copy.source}
                </caption>
                <thead>
                  <tr>
                    {columns.map((column) => (
                      <th key={column} scope="col">
                        {column}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {filtered.flatMap((trace) =>
                    REQUEST_TRACE_PHASES.map((phaseId) => {
                      const phase: RequestTracePhase = trace.phases.find(
                        (item) => item.id === phaseId,
                      ) ?? {
                        id: phaseId,
                        observed: false,
                        startMs: null,
                        endMs: null,
                        durationMs: null,
                      };
                      const cells = [
                        routeLabel(trace.route),
                        copy.outcomes[trace.outcome],
                        phaseName(phase, trace),
                        observation(phase),
                        offset(phase.startMs),
                        offset(phase.endMs),
                        offset(phase.durationMs),
                        trace.totalMs,
                        [phaseReason(phase), terminalLabel(trace)]
                          .filter(Boolean)
                          .join(" · "),
                        `${trace.peerAddress ?? copy.unknown} · ${trace.peerScope === "proxy" ? copy.peerProxy : copy.peerOrigin}`,
                        trace.statusCode ?? copy.unknown,
                        `${trace.bytesRead} B`,
                        trace.bodyLimitReached ? copy.yes : copy.no,
                      ];
                      return (
                        <tr key={`${trace.id}-${phaseId}`}>
                          <th scope="row">
                            {trace.targetLabel} · {trace.startedAt} · {trace.id}
                            <br />
                            <span className="text-muted">{trace.url}</span>
                          </th>
                          {cells.map((cell, index) => (
                            <td key={index}>{cell}</td>
                          ))}
                        </tr>
                      );
                    }),
                  )}
                </tbody>
              </table>
            </div>
          </details>
        </>
      ) : (
        <div className="viz-empty" role="status">
          <span className="viz-empty-mark" aria-hidden="true">
            —
          </span>
          <p>{traces.length ? copy.noMatches : copy.emptyTitle}</p>
          {traces.length === 0 && <p>{copy.emptyDetail}</p>}
        </div>
      )}
    </section>
  );
}

function DemoRequestWaterfall() {
  const [kind, setKind] = useState("all");
  const samples = useMemo(
    () =>
      requestSamples.filter(
        (request) => kind === "all" || request.kind === kind,
      ),
    [kind],
  );
  const total = (request: (typeof requestSamples)[number]) =>
    phases.reduce((sum, phase) => sum + request[phase], 0);
  const option = useMemo(
    () =>
      (p: ChartPalette): EChartsOption => ({
        ...baseOption(p),
        legend: {
          ...(baseOption(p).legend as object),
          data: phases.map((phase) => phaseLabels[phase]),
          selectedMode: false,
        },
        grid: { top: 46, right: 22, bottom: 60, left: 8, containLabel: true },
        xAxis: { type: "value", name: "ms", min: 0, ...axisStyle(p) },
        yAxis: {
          type: "category",
          inverse: true,
          data: samples.map((request) => `${request.id}  ${request.resource}`),
          ...axisStyle(p),
          axisLabel: {
            color: p.text,
            fontSize: 10,
            width: 110,
            overflow: "truncate",
          },
          splitLine: { show: false },
        },
        dataZoom: zoomOption(p),
        tooltip: {
          ...baseOption(p).tooltip,
          trigger: "axis",
          axisPointer: { type: "shadow" },
          formatter: (parameters) => {
            const item = Array.isArray(parameters) ? parameters[0] : parameters;
            const request = samples[item.dataIndex];
            if (!request) return "";
            return `${request.id} · ${request.resource}\n开始 +${request.start} ms · 总耗时 ${total(request)} ms\n${phases.map((phase) => `${phaseLabels[phase]}  ${request[phase]} ms`).join("\n")}`;
          },
        },
        series: [
          {
            type: "bar",
            name: "开始时间",
            stack: "request",
            data: samples.map((request) => request.start),
            silent: true,
            itemStyle: { color: "transparent" },
            emphasis: { disabled: true },
            tooltip: { show: false },
            barWidth: 14,
          },
          ...phases.map((phase) => ({
            type: "bar" as const,
            name: phaseLabels[phase],
            stack: "request",
            data: samples.map((request) => request[phase]),
            barWidth: 14,
            itemStyle: { color: p[phase] },
            emphasis: { focus: "series" as const },
          })),
        ],
      }),
    [samples],
  );
  const end = Math.max(
    ...samples.map((request) => request.start + total(request)),
  );
  return (
    <ChartFrame
      title={copy.demoTitle}
      subtitle={copy.demoSubtitle}
      demo
      unavailable="未接入请求追踪"
      summary={`${samples.length} 个样本请求 · 完成时间 +${end} ms`}
      controls={
        <ChartSelect
          label={strings.proxy.nodes.columns.protocol}
          value={kind}
          onChange={setKind}
        >
          <option value="all">全部请求</option>
          <option value="HTTP">HTTP</option>
          <option value="DNS">DNS</option>
        </ChartSelect>
      }
      hint={copy.demoHint}
      columns={[
        "请求",
        "开始 / ms",
        "DNS / ms",
        "TCP / ms",
        "TLS / ms",
        "TTFB / ms",
        "传输 / ms",
        "总计 / ms",
      ]}
      rows={samples.map((request) => [
        `${request.id} ${request.resource}`,
        request.start,
        ...phases.map((phase) => request[phase]),
        total(request),
      ])}
    >
      <p className="viz-summary">{copy.demoFictional}</p>
      <EChart
        option={option}
        label="按开始时间排列的请求阶段瀑布图，DNS、TCP、TLS、TTFB和传输阶段由不同色块表示"
        height={300}
      />
    </ChartFrame>
  );
}
