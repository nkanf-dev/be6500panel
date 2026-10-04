import { useEffect, useRef, useState } from "react";
import {
  Badge,
  Button,
  ErrorState,
  Field,
  Loading,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { ApiError, errorMessage, runRequest } from "../../lib/api";
import type { RuntimeController } from "../runtime/use-runtime";
import {
  localRulesApi,
  type LocalPolicy,
  type LocalRule,
  type LocalRulesPreview,
  type LocalRulesState,
  type Rule,
  type SubscriptionEdit,
} from "./local-rules-api";
import { PolicyReview } from "./policy-review";

const PAGE_SIZE = 20;
const kinds: { value: Rule["kind"]; label: string }[] = [
  { value: "domain", label: "精确域名" },
  { value: "domain-suffix", label: "域名后缀" },
  { value: "domain-keyword", label: "域名关键词" },
  { value: "ip-cidr", label: "IP-CIDR" },
  { value: "rule-set", label: "受控规则集" },
  { value: "match", label: "最终匹配 MATCH" },
];
const targets: { value: Rule["target"]; label: string }[] = [
  { value: "direct", label: "直连 DIRECT" },
  { value: "proxy", label: "代理 PROXY" },
  { value: "block", label: "阻断 BLOCK" },
];
const kindName = (value: Rule["kind"]) =>
  kinds.find((kind) => kind.value === value)?.label ?? value;
const actionName = (value: Rule["target"]) =>
  targets.find((target) => target.value === value)?.label ?? value;
function clonePolicy(policy: LocalPolicy): LocalPolicy {
  return {
    rules: policy.rules.map((entry) => ({ ...entry, rule: { ...entry.rule } })),
    subscriptionEdits: policy.subscriptionEdits.map((edit) => ({
      ...edit,
      ...(edit.replacement ? { replacement: { ...edit.replacement } } : {}),
    })),
  };
}
// Index is source display metadata, never a policy identity or a native rule index.
function policyIdentity(policy: LocalPolicy) {
  const rule = (entry: Rule) => ({
    kind: entry.kind,
    value: entry.value ?? "",
    target: entry.target,
    noResolve: entry.noResolve ?? false,
  });
  return JSON.stringify({
    rules: policy.rules.map((entry) => ({
      id: entry.id,
      enabled: entry.enabled,
      label: entry.label,
      note: entry.note,
      rule: rule(entry.rule),
    })),
    subscriptionEdits: policy.subscriptionEdits.map((edit) => ({
      id: edit.id,
      sourceFingerprint: edit.sourceFingerprint,
      disabled: edit.disabled,
      label: edit.label,
      note: edit.note,
      replacement: edit.replacement ? rule(edit.replacement) : null,
    })),
  });
}
function newID(prefix: string) {
  // getRandomValues also works on the router's local HTTP origin.
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  const random = Array.from(bytes, (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
  return `${prefix}-${random}`;
}
function newLocal(gpt = false): LocalRule {
  return {
    id: newID("local"),
    enabled: true,
    label: gpt ? "用户要求 · GPT 直连" : "",
    note: "",
    rule: {
      kind: "domain",
      value: gpt ? "gpt.kanglives.top" : "",
      target: "direct",
      index: 0,
    },
  };
}
function RuleFields({
  rule,
  disabled,
  onChange,
}: {
  rule: Rule;
  disabled: boolean;
  onChange: (rule: Rule) => void;
}) {
  return (
    <div
      className="form-grid"
      style={{
        gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 14rem), 1fr))",
      }}
    >
      <Field label="匹配类型">
        <select
          aria-label="匹配类型"
          className="select-trigger"
          value={rule.kind}
          disabled={disabled}
          onChange={(event) => {
            const kind = event.target.value as Rule["kind"];
            onChange({
              ...rule,
              kind,
              value:
                kind === "match"
                  ? ""
                  : kind === "rule-set"
                    ? "cn-domain"
                    : rule.kind === "match" || rule.kind === "rule-set"
                      ? ""
                      : rule.value,
              ...(kind !== "ip-cidr" ? { noResolve: false } : {}),
            });
          }}
        >
          {kinds.map((kind) => (
            <option key={kind.value} value={kind.value}>
              {kind.label}
            </option>
          ))}
        </select>
      </Field>
      <Field
        label="匹配值"
        hint={
          rule.kind === "domain"
            ? "只匹配这个域名，不包含子域名"
            : rule.kind === "match"
              ? "匹配所有剩余流量；后续规则不会参与分流"
              : undefined
        }
      >
        {rule.kind === "rule-set" ? (
          <select
            aria-label="匹配值"
            className="select-trigger"
            disabled={disabled}
            value={rule.value}
            onChange={(event) =>
              onChange({
                ...rule,
                value: event.target.value,
                ...(event.target.value !== "cn-ip" ? { noResolve: false } : {}),
              })
            }
          >
            {["cn-domain", "cn-ip", "proxy-domain"].map((tag) => (
              <option key={tag} value={tag}>
                {tag}
              </option>
            ))}
          </select>
        ) : (
          <input
            aria-label="匹配值"
            disabled={disabled || rule.kind === "match"}
            required={rule.kind !== "match"}
            maxLength={253}
            autoComplete="off"
            spellCheck={false}
            value={rule.value ?? ""}
            placeholder={
              rule.kind === "ip-cidr" ? "192.0.2.0/24" : "example.com"
            }
            onChange={(event) =>
              onChange({ ...rule, value: event.target.value })
            }
          />
        )}
      </Field>
      <Field label="动作">
        <select
          aria-label="动作"
          className="select-trigger"
          disabled={disabled}
          value={rule.target}
          onChange={(event) =>
            onChange({ ...rule, target: event.target.value as Rule["target"] })
          }
        >
          {targets.map((target) => (
            <option key={target.value} value={target.value}>
              {target.label}
            </option>
          ))}
        </select>
      </Field>
      {(rule.kind === "ip-cidr" ||
        (rule.kind === "rule-set" && rule.value === "cn-ip")) && (
        <label className="field">
          <span>
            <input
              aria-label="不主动解析域名"
              type="checkbox"
              disabled={disabled}
              checked={rule.noResolve ?? false}
              onChange={(event) =>
                onChange({ ...rule, noResolve: event.target.checked })
              }
            />{" "}
            不主动解析域名（no-resolve）
          </span>
        </label>
      )}
    </div>
  );
}
function Pagination({
  page,
  count,
  onPage,
  label,
  disabled = false,
}: {
  page: number;
  count: number;
  onPage: (page: number) => void;
  label: string;
  disabled?: boolean;
}) {
  const pages = Math.max(1, Math.ceil(count / PAGE_SIZE));
  return (
    <nav
      className="form-actions"
      style={{ flexWrap: "wrap" }}
      aria-label={`${label}分页`}
    >
      <Button
        type="button"
        size="small"
        disabled={disabled || page <= 1}
        onClick={() => onPage(page - 1)}
      >
        上一页
      </Button>
      <span className="text-muted text-xs">
        第 {page} / {pages} 页 · {count} 条 · 每页最多 {PAGE_SIZE} 条
      </span>
      <Button
        type="button"
        size="small"
        disabled={disabled || page >= pages}
        onClick={() => onPage(page + 1)}
      >
        下一页
      </Button>
    </nav>
  );
}
function Preview({
  value,
  current,
  policy,
}: {
  value: LocalRulesPreview;
  current: boolean;
  policy: LocalPolicy;
}) {
  const [requestedPage, setPage] = useState(1);
  const page = Math.min(
    requestedPage,
    Math.max(1, Math.ceil(value.rules.length / PAGE_SIZE)),
  );
  const edited = new Set(
    policy.subscriptionEdits.map((edit) => edit.sourceFingerprint),
  );
  const labels: Record<string, string> = {
    "orphaned-edit": "引用已失效 · 不参与应用",
    "disabled-rule": "已禁用 · 不参与应用",
    "unreachable-rule": "位于 MATCH 之后 · 不参与分流",
  };
  return (
    <details>
      <summary>
        合并预览 · {current ? "当前草稿" : "草稿已更改，请重新预览"} ·{" "}
        {value.rules.length} 条
      </summary>
      <div className="page-stack">
        <p className="text-muted text-xs">
          按本地规则、订阅规则顺序合并。网关管理与启动直连规则由运行配置保留在前；这里的顺序不是核心规则索引。
        </p>
        <div className="table-scroll">
          <table className="data-table" aria-label="合并规则预览">
            <thead>
              <tr>
                <th>合并顺序</th>
                <th>来源</th>
                <th>匹配</th>
                <th>动作</th>
                <th>标签</th>
              </tr>
            </thead>
            <tbody>
              {value.rules
                .slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE)
                .map((rule, offset) => {
                  const provenance =
                    value.provenance[(page - 1) * PAGE_SIZE + offset];
                  const source =
                    provenance?.layer === "local"
                      ? "本地规则"
                      : provenance?.sourceFingerprint &&
                          edited.has(provenance.sourceFingerprint)
                        ? "订阅改写"
                        : "订阅规则";
                  return (
                    <tr
                      key={provenance?.stableId ?? `preview-${page}-${offset}`}
                    >
                      <td>{(page - 1) * PAGE_SIZE + offset + 1}</td>
                      <td>{source}</td>
                      <td className="wrap">
                        {kindName(rule.kind)} · {rule.value || "全部剩余流量"}
                      </td>
                      <td>{actionName(rule.target)}</td>
                      <td>{provenance?.label || "—"}</td>
                    </tr>
                  );
                })}
            </tbody>
          </table>
        </div>
        <Pagination
          page={page}
          count={value.rules.length}
          onPage={setPage}
          label="合并预览"
        />
        {!!value.diagnostics.length && (
          <details>
            <summary>合并诊断 · {value.diagnostics.length} 条</summary>
            <ul aria-label="合并诊断">
              {value.diagnostics.map((diagnostic, index) => (
                <li
                  key={`${diagnostic.scope}-${diagnostic.index}-${diagnostic.code}-${index}`}
                >
                  {labels[diagnostic.code] ?? diagnostic.message}{" "}
                  <span className="mono text-muted text-xs">
                    {diagnostic.scope} · {diagnostic.code}
                  </span>
                </li>
              ))}
            </ul>
          </details>
        )}
      </div>
    </details>
  );
}

type RuntimeRefresh = Partial<
  Pick<
    RuntimeController,
    "refresh" | "pending" | "enabled" | "status" | "error"
  >
> & { reload?: () => void };
export function LocalRulesEditor({
  runtime,
  onPending,
}: {
  runtime?: RuntimeRefresh;
  onPending?: (pending: boolean) => void;
}) {
  const [observation, setObservation] = useState<LocalRulesState>();
  const [policy, setPolicy] = useState<LocalPolicy>();
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>();
  const [verified, setVerified] = useState(false);
  const [message, setMessage] = useState<string>();
  const [preview, setPreview] = useState<{
    value: LocalRulesPreview;
    identity: string;
    policy: LocalPolicy;
  }>();
  const [query, setQuery] = useState("");
  const [requestedPage, setPage] = useState(1);
  const [replacement, setReplacement] = useState<{
    fingerprint: string;
    rule: Rule;
  }>();
  const [acknowledgedRevision, setAcknowledgedRevision] = useState<string>();
  const [confirmation, setConfirmation] = useState<{
    revision: string;
    generation: number;
    reviewRevision?: string;
  }>();
  const active = useRef(true);
  const working = useRef(false);
  const policyRef = useRef<LocalPolicy | undefined>(undefined);
  policyRef.current = policy;
  const busy = pending || loading || !!runtime?.pending;
  const dirty =
    !!policy &&
    !!observation &&
    policyIdentity(policy) !== policyIdentity(observation.draft.policy);
  const review = observation?.policySummary;
  const acknowledged =
    !!review?.revision && acknowledgedRevision === review.revision;
  const policyReady = !review || review.omitted === 0 || acknowledged;
  const runtimeCurrent =
    runtime?.error === undefined &&
    (!runtime?.status ||
      runtime.status.generation === observation?.runtimeGeneration);
  const canApply =
    !!observation?.draft.revision &&
    verified &&
    runtimeCurrent &&
    !dirty &&
    !replacement &&
    !busy &&
    policyReady &&
    runtime?.enabled !== false;
  const applied =
    verified &&
    runtimeCurrent &&
    observation?.applied.state === "known" &&
    !!observation.applied.revision &&
    observation.applied.revision === observation.draft.revision &&
    observation.applied.generation === observation.runtimeGeneration;
  const appliedText =
    !verified || !runtimeCurrent || observation?.applied.state === "unknown"
      ? "运行规则：未确认"
      : applied
        ? "运行规则：已生效"
        : observation?.applied.state === "none"
          ? "运行规则：尚未应用"
          : "运行规则：已保存草稿待应用";
  const hasGPT = policy?.rules.some(
    (entry) =>
      entry.rule.kind === "domain" &&
      entry.rule.value?.toLowerCase() === "gpt.kanglives.top" &&
      entry.rule.target === "direct",
  );
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const filtered = (observation?.subscriptionRules ?? []).filter((entry) => {
    const text =
      `${entry.fingerprint} ${entry.rule.kind} ${kindName(entry.rule.kind)} ${entry.rule.value ?? ""} ${entry.rule.target} ${actionName(entry.rule.target)}`.toLowerCase();
    return terms.every((term) => text.includes(term));
  });
  const page = Math.min(
    requestedPage,
    Math.max(1, Math.ceil(filtered.length / PAGE_SIZE)),
  );
  const fingerprints = new Set(
    observation?.subscriptionRules.map((entry) => entry.fingerprint),
  );

  function acceptObservation(value: LocalRulesState) {
    setObservation(value);
    setVerified(true);
    setAcknowledgedRevision(undefined);
    setConfirmation(undefined);
  }
  useEffect(() => {
    const controller = new AbortController();
    let mounted = true;
    active.current = true;
    runRequest(localRulesApi.state(), controller.signal)
      .then((value) => {
        if (!mounted) return;
        acceptObservation(value);
        const next = clonePolicy(value.draft.policy);
        setPolicy(
          !next.rules.length && !next.subscriptionEdits.length
            ? { ...next, rules: [newLocal(true)] }
            : next,
        );
        setPreview({
          value: value.preview,
          identity: policyIdentity(value.draft.policy),
          policy: clonePolicy(value.draft.policy),
        });
      })
      .catch((cause) => {
        if (mounted) {
          setError(cause);
          setVerified(false);
        }
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });
    return () => {
      mounted = false;
      active.current = false;
      controller.abort();
    };
  }, []);

  function change(next: LocalPolicy) {
    setPolicy(next);
    setConfirmation(undefined);
    setMessage(undefined);
  }
  function updateLocal(id: string, update: Partial<LocalRule>) {
    if (!policy || busy) return;
    change({
      ...policy,
      rules: policy.rules.map((entry) =>
        entry.id === id ? { ...entry, ...update } : entry,
      ),
    });
  }
  function move(id: string, direction: number) {
    if (!policy || busy) return;
    const index = policy.rules.findIndex((entry) => entry.id === id);
    if (
      index < 0 ||
      index + direction < 0 ||
      index + direction >= policy.rules.length
    )
      return;
    const rules = [...policy.rules];
    [rules[index], rules[index + direction]] = [
      rules[index + direction],
      rules[index],
    ];
    change({ ...policy, rules });
  }
  function add(gpt = false) {
    if (policy && !busy && policy.rules.length < 512)
      change({
        ...policy,
        rules: gpt
          ? [newLocal(true), ...policy.rules]
          : [...policy.rules, newLocal()],
      });
  }
  function restore(fingerprint: string) {
    if (!policy || busy) return;
    change({
      ...policy,
      subscriptionEdits: policy.subscriptionEdits.filter(
        (edit) => edit.sourceFingerprint !== fingerprint,
      ),
    });
    setReplacement(undefined);
  }
  function editSubscription(fingerprint: string, rule?: Rule) {
    if (!policy || busy) return;
    const existing = policy.subscriptionEdits.find(
      (edit) => edit.sourceFingerprint === fingerprint,
    );
    if (!existing && policy.subscriptionEdits.length >= 1024) return;
    const edit: SubscriptionEdit = {
      id: existing?.id ?? newID("edit"),
      sourceFingerprint: fingerprint,
      disabled: !rule,
      label: existing?.label ?? "",
      note: existing?.note ?? "",
      ...(rule ? { replacement: { ...rule } } : {}),
    };
    change({
      ...policy,
      subscriptionEdits: existing
        ? policy.subscriptionEdits.map((entry) =>
            entry.sourceFingerprint === fingerprint ? edit : entry,
          )
        : [...policy.subscriptionEdits, edit],
    });
    setReplacement(undefined);
  }
  async function operation(kind: "save" | "preview" | "refresh" | "apply") {
    if (
      working.current ||
      busy ||
      (kind !== "refresh" && !policy) ||
      (kind === "apply" && (!canApply || !confirmation))
    )
      return;
    working.current = true;
    setPending(true);
    onPending?.(true);
    setError(undefined);
    setMessage(undefined);
    const submitted = policy ? clonePolicy(policy) : undefined;
    try {
      if (kind === "preview" && submitted) {
        const value = await runRequest(localRulesApi.preview(submitted));
        if (active.current) {
          setPreview({
            value,
            identity: policyIdentity(submitted),
            policy: submitted,
          });
          setMessage("预览已更新；未保存，也未应用。");
        }
      } else if (kind === "save" && submitted) {
        const value = await runRequest(localRulesApi.save(submitted));
        if (active.current) {
          acceptObservation(value);
          setPreview({
            value: value.preview,
            identity: policyIdentity(value.draft.policy),
            policy: clonePolicy(value.draft.policy),
          });
          if (policyIdentity(value.draft.policy) !== policyIdentity(submitted))
            throw new ApiError({
              code: "save_readback_mismatch",
              message: "保存回读与当前草稿不同，请刷新后核对",
            });
          setPolicy(clonePolicy(value.draft.policy));
          setMessage("草稿已保存；尚未应用本次保存。");
        }
      } else {
        if (kind === "apply" && confirmation) {
          const body = {
            revision: confirmation.revision,
            generation: confirmation.generation,
            ...(acknowledged && confirmation.reviewRevision
              ? { acknowledgedRevision: confirmation.reviewRevision }
              : {}),
          };
          setVerified(false);
          setConfirmation(undefined);
          await runRequest(localRulesApi.apply(body));
        }
        const value = await runRequest(localRulesApi.state());
        if (active.current) {
          acceptObservation(value);
          if (!policyRef.current) {
            const next = clonePolicy(value.draft.policy);
            setPolicy(
              !next.rules.length && !next.subscriptionEdits.length
                ? { ...next, rules: [newLocal(true)] }
                : next,
            );
          }
          setPreview({
            value: value.preview,
            identity: policyIdentity(value.draft.policy),
            policy: clonePolicy(value.draft.policy),
          });
          setMessage(
            kind === "apply"
              ? "已读取运行状态；生效结果以回读状态为准。"
              : "已刷新保存与运行状态；当前编辑内容保留。",
          );
        }
        if (kind === "apply") {
          if (runtime?.reload) runtime.reload();
          else runtime?.refresh?.();
        }
      }
    } catch (cause) {
      if (active.current) {
        setError(cause);
        if (kind !== "preview") setVerified(false);
        setConfirmation(undefined);
      }
    } finally {
      working.current = false;
      if (active.current) setPending(false);
      onPending?.(false);
    }
  }

  return (
    <Panel aria-label="自定义规则编辑器">
      <PanelHeader
        title="自定义规则"
        subtitle="本地规则优先，订阅编辑按稳定指纹保留；订阅更新不会覆盖本地规则"
        action={
          <Button
            type="button"
            size="small"
            disabled={busy}
            onClick={() => void operation("refresh")}
          >
            刷新规则状态
          </Button>
        }
      />
      <div className="config-form page-stack">
        <div
          className="form-actions"
          style={{ flexWrap: "wrap" }}
          aria-label="规则保存与生效状态"
        >
          <Badge tone={dirty ? "warning" : "neutral"}>
            {!observation
              ? "草稿：读取中"
              : dirty
                ? "草稿：未保存更改"
                : "草稿：已保存"}
          </Badge>
          <Badge
            data-testid="rules-applied-state"
            tone={applied ? "success" : "neutral"}
          >
            {appliedText}
          </Badge>
        </div>
        <p className="text-muted text-xs">
          编辑仅保留在当前页面。预览不写入；保存只保存草稿；应用才会更新运行配置。
        </p>
        {loading && <Loading label="正在读取规则" />}
        {error !== undefined && (
          <ErrorState
            message={errorMessage(error)}
            onRetry={busy ? undefined : () => void operation("refresh")}
          />
        )}
        {policy && (
          <>
            <div className="form-actions" style={{ flexWrap: "wrap" }}>
              <h3>本地规则 · {policy.rules.length}</h3>
              <div className="form-actions" style={{ flexWrap: "wrap" }}>
                <Button
                  type="button"
                  size="small"
                  disabled={busy || policy.rules.length >= 512}
                  onClick={() => add()}
                >
                  添加域名直连规则
                </Button>
                {!hasGPT && (
                  <Button
                    type="button"
                    size="small"
                    disabled={busy || policy.rules.length >= 512}
                    onClick={() => add(true)}
                  >
                    加入 gpt.kanglives.top 直连
                  </Button>
                )}
              </div>
            </div>
            {!policy.rules.length && (
              <p className="text-muted">
                暂无本地规则。添加规则后，先保存，再按需应用。
              </p>
            )}
            {policy.rules.map((entry, index) => (
              <fieldset
                key={entry.id}
                aria-label={`本地规则 ${entry.id}`}
                disabled={busy}
                className="page-stack"
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: "var(--radius-md)",
                  padding: "1rem",
                  minWidth: 0,
                }}
              >
                <legend>
                  {index + 1}. {entry.label || entry.rule.value || "新规则"}
                </legend>
                <div className="form-actions" style={{ flexWrap: "wrap" }}>
                  <label>
                    <input
                      aria-label="启用规则"
                      type="checkbox"
                      checked={entry.enabled}
                      onChange={(event) =>
                        updateLocal(entry.id, { enabled: event.target.checked })
                      }
                    />{" "}
                    启用规则
                  </label>
                  <div className="form-actions" style={{ flexWrap: "wrap" }}>
                    <Button
                      type="button"
                      size="small"
                      aria-label="上移规则"
                      disabled={busy || index === 0}
                      onClick={() => move(entry.id, -1)}
                    >
                      上移
                    </Button>
                    <Button
                      type="button"
                      size="small"
                      aria-label="下移规则"
                      disabled={busy || index === policy.rules.length - 1}
                      onClick={() => move(entry.id, 1)}
                    >
                      下移
                    </Button>
                    <Button
                      type="button"
                      size="small"
                      aria-label="删除规则"
                      disabled={busy}
                      onClick={() =>
                        change({
                          ...policy,
                          rules: policy.rules.filter(
                            (rule) => rule.id !== entry.id,
                          ),
                        })
                      }
                    >
                      删除
                    </Button>
                  </div>
                </div>
                <RuleFields
                  rule={entry.rule}
                  disabled={busy}
                  onChange={(rule) => updateLocal(entry.id, { rule })}
                />
                <Field label="标签">
                  <input
                    aria-label="标签"
                    maxLength={64}
                    value={entry.label}
                    onChange={(event) =>
                      updateLocal(entry.id, { label: event.target.value })
                    }
                  />
                </Field>
                <details>
                  <summary>备注与规则标识</summary>
                  <Field label="备注">
                    <textarea
                      aria-label="备注"
                      maxLength={256}
                      rows={2}
                      value={entry.note}
                      onChange={(event) =>
                        updateLocal(entry.id, { note: event.target.value })
                      }
                    />
                  </Field>
                  <p className="mono text-muted text-xs">{entry.id}</p>
                </details>
              </fieldset>
            ))}
            <details open>
              <summary>
                订阅规则 · {observation?.subscriptionRules.length ?? 0} 条
              </summary>
              <div className="page-stack">
                <p className="text-muted text-xs">
                  禁用或改写仅保存在本地草稿；订阅规则更新时，只要规则内容未变，禁用与改写依然有效。
                </p>
                <Field
                  label="搜索订阅规则"
                  hint="搜索匹配类型、值、动作或完整指纹"
                >
                  <input
                    type="search"
                    aria-label="搜索订阅规则"
                    value={query}
                    onChange={(event) => {
                      setQuery(event.target.value);
                      setPage(1);
                    }}
                  />
                </Field>
                <div className="table-scroll">
                  <table className="data-table" aria-label="订阅规则">
                    <thead>
                      <tr>
                        <th>匹配</th>
                        <th>动作</th>
                        <th>草稿编辑</th>
                        <th>操作</th>
                      </tr>
                    </thead>
                    <tbody>
                      {filtered
                        .slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE)
                        .map((entry) => {
                          const edit = policy.subscriptionEdits.find(
                            (item) =>
                              item.sourceFingerprint === entry.fingerprint,
                          );
                          return (
                            <tr key={entry.fingerprint}>
                              <td className="wrap">
                                {kindName(entry.rule.kind)}
                                <br />
                                <span className="mono">
                                  {entry.rule.value || "全部剩余流量"}
                                </span>
                                <details>
                                  <summary>来源指纹</summary>
                                  <span className="mono text-xs wrap">
                                    {entry.fingerprint}
                                  </span>
                                </details>
                              </td>
                              <td>{actionName(entry.rule.target)}</td>
                              <td>
                                {edit?.disabled
                                  ? "已禁用（草稿）"
                                  : edit?.replacement
                                    ? "已改写（草稿）"
                                    : "原始订阅"}
                              </td>
                              <td>
                                <div
                                  className="form-actions"
                                  style={{ flexWrap: "wrap" }}
                                >
                                  <Button
                                    type="button"
                                    size="small"
                                    aria-label="禁用订阅规则"
                                    disabled={
                                      busy ||
                                      !!edit?.disabled ||
                                      (!edit &&
                                        policy.subscriptionEdits.length >= 1024)
                                    }
                                    onClick={() =>
                                      editSubscription(entry.fingerprint)
                                    }
                                  >
                                    禁用
                                  </Button>
                                  <Button
                                    type="button"
                                    size="small"
                                    aria-label="改写订阅规则"
                                    disabled={
                                      busy ||
                                      (!edit &&
                                        policy.subscriptionEdits.length >= 1024)
                                    }
                                    onClick={() =>
                                      setReplacement({
                                        fingerprint: entry.fingerprint,
                                        rule: {
                                          ...(edit?.replacement ?? entry.rule),
                                        },
                                      })
                                    }
                                  >
                                    改写
                                  </Button>
                                  {edit && (
                                    <Button
                                      type="button"
                                      size="small"
                                      aria-label="恢复订阅规则"
                                      disabled={busy}
                                      onClick={() => restore(entry.fingerprint)}
                                    >
                                      恢复
                                    </Button>
                                  )}
                                </div>
                              </td>
                            </tr>
                          );
                        })}
                    </tbody>
                  </table>
                </div>
                {!filtered.length && (
                  <p className="text-muted">没有匹配的订阅规则。</p>
                )}
                <Pagination
                  page={page}
                  count={filtered.length}
                  onPage={setPage}
                  label="订阅规则"
                  disabled={busy}
                />
              </div>
            </details>
            {replacement && (
              <fieldset
                aria-label="订阅改写"
                disabled={busy}
                className="page-stack"
              >
                <legend>订阅改写 · 未加入草稿</legend>
                <p className="mono text-xs wrap">{replacement.fingerprint}</p>
                <RuleFields
                  rule={replacement.rule}
                  disabled={busy}
                  onChange={(rule) => setReplacement({ ...replacement, rule })}
                />
                <div className="form-actions" style={{ flexWrap: "wrap" }}>
                  <Button
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      editSubscription(
                        replacement.fingerprint,
                        replacement.rule,
                      )
                    }
                  >
                    加入改写草稿
                  </Button>
                  <Button
                    type="button"
                    disabled={busy}
                    onClick={() => setReplacement(undefined)}
                  >
                    取消改写
                  </Button>
                </div>
              </fieldset>
            )}
            {!!policy.subscriptionEdits.length && (
              <details open>
                <summary>
                  订阅编辑 · {policy.subscriptionEdits.length} 条
                </summary>
                <ul className="page-stack" aria-label="订阅编辑记录">
                  {policy.subscriptionEdits.map((edit) => (
                    <li key={edit.id} className="page-stack">
                      <strong>
                        {edit.label ||
                          (edit.disabled ? "禁用订阅规则" : "改写订阅规则")}
                      </strong>
                      <span>
                        {fingerprints.has(edit.sourceFingerprint)
                          ? "当前订阅引用有效"
                          : "引用已失效 · 不参与应用"}
                      </span>
                      <span className="mono text-xs wrap">
                        {edit.sourceFingerprint}
                      </span>
                      {edit.replacement && (
                        <span>
                          {kindName(edit.replacement.kind)} ·{" "}
                          {edit.replacement.value || "全部剩余流量"} →{" "}
                          {actionName(edit.replacement.target)}
                        </span>
                      )}
                      <details>
                        <summary>标签与备注</summary>
                        <Field label="编辑标签">
                          <input
                            disabled={busy}
                            maxLength={64}
                            value={edit.label}
                            onChange={(event) =>
                              change({
                                ...policy,
                                subscriptionEdits: policy.subscriptionEdits.map(
                                  (entry) =>
                                    entry.id === edit.id
                                      ? { ...entry, label: event.target.value }
                                      : entry,
                                ),
                              })
                            }
                          />
                        </Field>
                        <Field label="编辑备注">
                          <textarea
                            disabled={busy}
                            maxLength={256}
                            rows={2}
                            value={edit.note}
                            onChange={(event) =>
                              change({
                                ...policy,
                                subscriptionEdits: policy.subscriptionEdits.map(
                                  (entry) =>
                                    entry.id === edit.id
                                      ? { ...entry, note: event.target.value }
                                      : entry,
                                ),
                              })
                            }
                          />
                        </Field>
                      </details>
                      <Button
                        type="button"
                        size="small"
                        aria-label="移除订阅编辑"
                        disabled={busy}
                        onClick={() => restore(edit.sourceFingerprint)}
                      >
                        移除本地编辑
                      </Button>
                    </li>
                  ))}
                </ul>
              </details>
            )}
            {preview && (
              <Preview
                value={preview.value}
                policy={preview.policy}
                current={preview.identity === policyIdentity(policy)}
              />
            )}
            <PolicyReview
              summary={review}
              acknowledgment={{
                checked: acknowledged,
                disabled: busy,
                onChange: (checked) => {
                  setAcknowledgedRevision(
                    checked ? review?.revision : undefined,
                  );
                  setConfirmation(undefined);
                },
              }}
            />
            <div className="form-actions" style={{ flexWrap: "wrap" }}>
              <Button
                type="button"
                disabled={busy}
                onClick={() => void operation("preview")}
              >
                预览合并规则
              </Button>
              <Button
                type="button"
                variant="primary"
                disabled={busy || !dirty}
                onClick={() => void operation("save")}
              >
                保存草稿
              </Button>
              <Button
                type="button"
                variant={!dirty && canApply ? "primary" : "secondary"}
                disabled={!canApply}
                onClick={() => {
                  if (canApply && observation)
                    setConfirmation({
                      revision: observation.draft.revision,
                      generation: observation.runtimeGeneration,
                      ...(acknowledged
                        ? { reviewRevision: review?.revision }
                        : {}),
                    });
                }}
              >
                应用已保存规则
              </Button>
            </div>
            <p className="text-muted text-xs">
              应用会更新运行配置，现有连接可能重新建立
            </p>
            {dirty && (
              <p className="text-muted text-xs">
                有未保存更改。先保存草稿，再应用已保存版本。
              </p>
            )}
            {(!verified || !runtimeCurrent) && !loading && (
              <p className="text-muted text-xs">请刷新规则状态后再应用。</p>
            )}
          </>
        )}
        {message && <p role="status">{message}</p>}
        {confirmation && (
          <section
            role="dialog"
            aria-modal="false"
            aria-label="确认应用规则"
            className="page-stack"
          >
            <h3>确认应用已保存规则？</h3>
            <p>应用会更新运行配置，现有连接可能重新建立</p>
            <p className="text-muted text-xs">
              应用当前已保存草稿，不会保存其他编辑。
            </p>
            <div className="form-actions" style={{ flexWrap: "wrap" }}>
              <Button
                type="button"
                variant="primary"
                disabled={!canApply}
                onClick={() => void operation("apply")}
              >
                确认应用
              </Button>
              <Button
                type="button"
                disabled={pending}
                onClick={() => setConfirmation(undefined)}
              >
                取消
              </Button>
            </div>
          </section>
        )}
      </div>
    </Panel>
  );
}
