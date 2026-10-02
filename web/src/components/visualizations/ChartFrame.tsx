import { strings } from "../../locales/strings";
import { useId, type ReactNode } from 'react';
export interface VisualizationProps { demo?: boolean }
export interface TableColumn { label: string }
interface Props {
  title: string; subtitle: string; demo: boolean; unavailable: string;
  summary?: string; controls?: ReactNode; children?: ReactNode; hint?: string;
  hasData?: boolean; source?: string; emptyLabel?: string;
  columns?: readonly string[]; rows?: readonly (readonly (string | number)[])[];
}
export function ChartFrame({ title, subtitle, demo, unavailable, summary, controls, children, hint, hasData = demo, source = '接口采样', emptyLabel = strings.dashboard.states.unavailable, columns = [], rows = [] }: Props) {
  const id = useId();
  const sourceLabel = demo ? '固定样本' : source;
  return <section className="viz-panel" aria-labelledby={`${id}-title`}>
    <header className="viz-header">
      <div><h3 id={`${id}-title`}>{title}</h3><p className="viz-subtitle">{subtitle}</p></div>
      <span className={`viz-source${demo ? ' viz-source-demo' : ''}`}>{demo ? strings.dashboard.states.demo : hasData ? source : emptyLabel}</span>
    </header>
    {hasData ? <>
      {controls && <div className="viz-controls">{controls}</div>}
      <p className="viz-summary">{summary}</p>
      {children}
      <footer className="viz-footer"><span>来源：{sourceLabel}</span>{hint && <span>{hint}</span>}</footer>
      <details className="viz-table-details"><summary>查看数据表 <span>{rows.length} 条</span></summary>
        <div className="viz-table-scroll"><table><caption>{title} · {demo ? strings.dashboard.states.demo : source}</caption>
          <thead><tr>{columns.map(column => <th key={column} scope="col">{column}</th>)}</tr></thead>
          <tbody>{rows.map((row, i) => <tr key={i}>{row.map((cell, j) => j === 0 ? <th key={j} scope="row">{cell}</th> : <td key={j}>{cell}</td>)}</tr>)}</tbody>
        </table></div>
      </details>
    </> : <div className="viz-empty" role="status"><span className="viz-empty-mark" aria-hidden="true">—</span><p>{unavailable}</p></div>}
  </section>;
}
export function ChartSelect({ label, value, onChange, children }: { label: string; value: string; onChange: (value: string) => void; children: ReactNode }) {
  const id = useId();
  return <label className="viz-filter" htmlFor={id}><span>{label}</span><select id={id} aria-label={label} value={value} onChange={event => onChange(event.target.value)}>{children}</select></label>;
}
