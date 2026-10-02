import { useMemo, useState } from "react";
import { FileText, RefreshCw, Search } from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Loading,
  Panel,
  PanelHeader,
  Select,
} from "../components/ui/primitives";
import { api, errorMessage } from "../lib/api";
import { useResource } from "../lib/use-resource";
import { timestamp } from "../lib/format";
export function LogsPanel() {
  const { data, error, loading, reload } = useResource(api.logs);
  const [query, setQuery] = useState("");
  const [level, setLevel] = useState("all");
  const [module, setModule] = useState("all");
  const moduleOptions = useMemo(
    () => [...new Set(data?.entries.map((entry) => entry.module) ?? [])].sort(),
    [data],
  );
  const entries =
    data?.entries
      .filter(
        (entry) =>
          (level === "all" || entry.level === level) &&
          (module === "all" || entry.module === module) &&
          `${entry.code} ${entry.message} ${entry.module}`
            .toLowerCase()
            .includes(query.toLowerCase()),
      )
      .slice()
      .reverse() ?? [];
  return (
    <Panel>
      <PanelHeader
        title="运行日志"
        subtitle={`内存缓冲 · 最近 100 条 / 容量 ${data?.capacity ?? "—"}`}
        action={
          <Button size="small" onClick={reload} disabled={loading}>
            <RefreshCw size={14} className={loading ? "spin" : ""} />
            刷新
          </Button>
        }
      />
      <div className="logs-toolbar">
        <label className="search-field">
          <Search size={14} />
          <input
            aria-label="筛选日志"
            placeholder="筛选 code / module / message…"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <Select
          label="日志级别"
          value={level}
          onValueChange={setLevel}
          options={[
            { value: "all", label: "全部级别" },
            ...["DEBUG", "INFO", "WARN", "ERROR"].map((value) => ({
              value,
              label: value,
            })),
          ]}
        />
        <Select
          label="日志模块"
          value={module}
          onValueChange={setModule}
          options={[
            { value: "all", label: "全部模块" },
            ...moduleOptions.map((value) => ({ value, label: value })),
          ]}
        />
      </div>
      {error !== undefined && (
        <ErrorState message={errorMessage(error)} onRetry={reload} />
      )}
      {loading && !data ? (
        <Loading />
      ) : (
        <div className="table-scroll">
          <table className="data-table logs-table">
            <thead>
              <tr>
                <th>时间</th>
                <th>级别</th>
                <th>模块 / code</th>
                <th>消息</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr key={entry.sequence}>
                  <td className="mono text-muted">{timestamp(entry.time)}</td>
                  <td>
                    <Badge
                      tone={
                        entry.level === "ERROR"
                          ? "danger"
                          : entry.level === "WARN"
                            ? "warning"
                            : "neutral"
                      }
                    >
                      {entry.level}
                    </Badge>
                  </td>
                  <td>
                    <span className="log-code">
                      <small>{entry.module}</small>
                      <code>{entry.code}</code>
                    </span>
                  </td>
                  <td className="wrap">{entry.message}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {!entries.length && (
            <EmptyState
              icon={<FileText size={22} />}
              title={
                query || level !== "all" || module !== "all"
                  ? "无匹配日志"
                  : "暂无日志"
              }
            />
          )}
        </div>
      )}
      <div className="table-footer">
        <span>{entries.length} 条记录</span>
        <span>最新记录在前</span>
      </div>
    </Panel>
  );
}
