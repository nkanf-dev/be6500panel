import { useState } from "react";
import { Badge, Button } from "../../components/ui/primitives";
import type { ProxyPolicySummary } from "./policy-contracts";

const OMISSION_PAGE_SIZE = 20;
const processRuleExplanation =
  "PROCESS-NAME / PROCESS-PATH：路由器网关无法识别转发的 LAN 客户端应用进程，不支持进程分流。";
function omissionMessage(code: string, message: string) {
  switch (code) {
    case "unsupported-process-rule":
      return processRuleExplanation;
    case "unsupported-rule":
      return "此规则类型或规则集不受网关支持，不会加入路由配置。";
    case "unreachable-rule":
      return "此规则位于最终匹配规则之后，不会参与分流。";
    case "unknown-rule-target":
      return "规则目标不是已知节点或策略组，不会加入路由配置。";
    case "invalid-rule":
      return "规则格式或选项无效，不会加入路由配置。";
    default:
      return message;
  }
}

function PolicyOmissions({ summary }: { summary: ProxyPolicySummary }) {
  const [open, setOpen] = useState(false);
  const [requestedPage, setPage] = useState(1);
  const pageCount = Math.max(
    1,
    Math.ceil(summary.omittedRules.length / OMISSION_PAGE_SIZE),
  );
  const page = Math.min(requestedPage, pageCount);
  const rules = summary.omittedRules.slice(
    (page - 1) * OMISSION_PAGE_SIZE,
    page * OMISSION_PAGE_SIZE,
  );
  return (
    <details onToggle={(event) => setOpen(event.currentTarget.open)}>
      <summary>查看忽略规则明细</summary>
      {open && (
        <div className="page-stack">
          {!!summary.reasons.length && (
            <ul aria-label="忽略原因分类" className="text-xs">
              {summary.reasons.map((reason) => (
                <li key={reason.code}>
                  {omissionMessage(reason.code, reason.message)} · 分类数量：
                  {reason.count}
                </li>
              ))}
            </ul>
          )}
          <div
            className="table-scroll"
            style={{ maxHeight: "16rem", overflowY: "auto" }}
          >
            <table className="data-table" aria-label="忽略规则明细">
              <thead>
                <tr>
                  <th>订阅规则序号</th>
                  <th>状态码</th>
                  <th>忽略原因</th>
                </tr>
              </thead>
              <tbody>
                {rules.map((rule) => (
                  <tr key={`${rule.index}-${rule.code}`}>
                    <td>{rule.index + 1}</td>
                    <td className="mono">{rule.code}</td>
                    <td className="wrap">
                      {omissionMessage(rule.code, rule.message)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {summary.omittedRules.length ? (
            <nav aria-label="忽略规则分页" className="form-actions">
              <Button
                type="button"
                size="small"
                aria-label="上一页忽略规则"
                disabled={page <= 1}
                onClick={() => setPage(page - 1)}
              >
                上一页
              </Button>
              <span className="text-muted text-xs" role="status">
                第 {page} / {pageCount} 页 · 每页最多 {OMISSION_PAGE_SIZE} 条
              </span>
              <Button
                type="button"
                size="small"
                aria-label="下一页忽略规则"
                disabled={page >= pageCount}
                onClick={() => setPage(page + 1)}
              >
                下一页
              </Button>
            </nav>
          ) : (
            <p className="text-muted text-xs">当前响应未提供忽略规则明细。</p>
          )}
        </div>
      )}
    </details>
  );
}

/** Read-only by default. Only the node Apply form supplies acknowledgment. */
export function PolicyReview({
  summary,
  acknowledgment,
}: {
  summary?: ProxyPolicySummary;
  acknowledgment?: {
    checked: boolean;
    disabled?: boolean;
    onChange: (checked: boolean) => void;
  };
}) {
  const hasProcessRules =
    summary?.reasons.some(
      (reason) => reason.code === "unsupported-process-rule",
    ) ||
    summary?.omittedRules.some(
      (rule) => rule.code === "unsupported-process-rule",
    );
  return (
    <section aria-label="路由规则审阅" className="page-stack">
      {summary ? (
        <>
          <strong className="text-xs">
            共 {summary.total} 条规则 · {summary.supported} 条可应用路由规则
          </strong>
          <p className="text-muted text-xs">
            统计来自当前订阅的解析结果；可应用表示支持生成路由配置，不代表实时分流结果。
          </p>
          {summary.omitted > 0 && (
            <>
              <div>
                <Badge tone="warning">{summary.omitted} 条规则将忽略</Badge>
              </div>
              {hasProcessRules && (
                <p className="text-muted text-xs">{processRuleExplanation}</p>
              )}
              <PolicyOmissions key={summary.revision} summary={summary} />
              {acknowledgment && (
                <label className="field">
                  <span>
                    <input
                      type="checkbox"
                      checked={acknowledgment.checked}
                      disabled={acknowledgment.disabled || !summary.revision}
                      onChange={(event) =>
                        acknowledgment.onChange(event.target.checked)
                      }
                    />{" "}
                    我已了解上述规则将被忽略，并继续应用当前路由策略
                  </span>
                  <span className="field-hint">
                    仅确认当前规则版本；勾选不会保存或应用，规则变化后需重新审阅。
                  </span>
                </label>
              )}
            </>
          )}
        </>
      ) : (
        <>
          <strong className="text-xs">路由规则统计未知</strong>
          <p className="text-muted text-xs">
            当前响应未提供规则统计，无法确认可应用或忽略的规则数量。
          </p>
        </>
      )}
    </section>
  );
}
