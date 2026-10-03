import { strings } from "../../locales/strings";
import { useId, useState, type ReactNode } from "react";
export interface VisualizationProps {
  demo?: boolean;
}
export interface TableColumn {
  label: string;
}
interface Props {
  title: string;
  subtitle: string;
  demo: boolean;
  unavailable: string;
  summary?: string;
  controls?: ReactNode;
  children?: ReactNode;
  hint?: string;
  hasData?: boolean;
  source?: string;
  emptyLabel?: string;
  columns?: readonly string[];
  rows?: readonly (readonly (string | number)[])[];
  tablePageSize?: number;
  lazyTable?: boolean;
}
export function ChartFrame({
  title,
  subtitle,
  demo,
  unavailable,
  summary,
  controls,
  children,
  hint,
  hasData = demo,
  source = "接口采样",
  emptyLabel = strings.dashboard.states.unavailable,
  columns = [],
  rows = [],
  tablePageSize,
  lazyTable = false,
}: Props) {
  const id = useId();
  const sourceLabel = demo ? "固定样本" : source;
  const [tableOpen, setTableOpen] = useState(false);
  const [tablePage, setTablePage] = useState(0);
  const pageSize = tablePageSize
    ? Math.max(1, Math.min(100, Math.floor(tablePageSize)))
    : Math.max(1, rows.length);
  const pageCount = Math.max(1, Math.ceil(rows.length / pageSize));
  const page = Math.min(tablePage, pageCount - 1);
  const visibleRows = rows.slice(page * pageSize, (page + 1) * pageSize);
  return (
    <section className="viz-panel" aria-labelledby={`${id}-title`}>
      <header className="viz-header">
        <div>
          <h3 id={`${id}-title`}>{title}</h3>
          <p className="viz-subtitle">{subtitle}</p>
        </div>
        <span className={`viz-source${demo ? " viz-source-demo" : ""}`}>
          {demo ? strings.dashboard.states.demo : hasData ? source : emptyLabel}
        </span>
      </header>
      {hasData ? (
        <>
          {controls && <div className="viz-controls">{controls}</div>}
          <p className="viz-summary">{summary}</p>
          {children}
          <footer className="viz-footer">
            <span>来源：{sourceLabel}</span>
            {hint && <span>{hint}</span>}
          </footer>
          <details
            className="viz-table-details"
            onToggle={(event) => setTableOpen(event.currentTarget.open)}
          >
            <summary>
              查看数据表 <span>{rows.length} 条</span>
            </summary>
            {(!lazyTable || tableOpen) && (
              <>
                <div className="viz-table-scroll">
                  <table>
                    <caption>
                      {title} · {demo ? strings.dashboard.states.demo : source}
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
                      {visibleRows.map((row, i) => (
                        <tr key={page * pageSize + i}>
                          {row.map((cell, j) =>
                            j === 0 ? (
                              <th key={j} scope="row">
                                {cell}
                              </th>
                            ) : (
                              <td key={j}>{cell}</td>
                            ),
                          )}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                {tablePageSize && rows.length > pageSize && (
                  <nav
                    className="viz-table-pagination"
                    aria-label={`${title} 数据表分页`}
                  >
                    <button
                      type="button"
                      aria-label={`${title} 数据表上一页`}
                      disabled={page === 0}
                      onClick={() => setTablePage(page - 1)}
                    >
                      上一页
                    </button>
                    <span>
                      第 {page + 1} / {pageCount} 页 · 共 {rows.length} 条
                    </span>
                    <button
                      type="button"
                      aria-label={`${title} 数据表下一页`}
                      disabled={page >= pageCount - 1}
                      onClick={() => setTablePage(page + 1)}
                    >
                      下一页
                    </button>
                    <button
                      type="button"
                      aria-label={`${title} 数据表末页`}
                      disabled={page >= pageCount - 1}
                      onClick={() => setTablePage(pageCount - 1)}
                    >
                      末页
                    </button>
                  </nav>
                )}
              </>
            )}
          </details>
        </>
      ) : (
        <div className="viz-empty" role="status">
          <span className="viz-empty-mark" aria-hidden="true">
            —
          </span>
          <p>{unavailable}</p>
        </div>
      )}
    </section>
  );
}
export function ChartSelect({
  label,
  value,
  onChange,
  children,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <label className="viz-filter" htmlFor={id}>
      <span>{label}</span>
      <select
        id={id}
        aria-label={label}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        {children}
      </select>
    </label>
  );
}
