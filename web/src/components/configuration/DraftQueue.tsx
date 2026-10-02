import { useState } from "react";
import { FileDiff, Trash2 } from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  Panel,
  PanelHeader,
} from "../ui/primitives";
import type { ConfigurationController } from "./use-configuration";
import type { ConfigurationDraft } from "./contracts";

export function DraftDiff({ draft }: { draft: ConfigurationDraft }) {
  return (
    <div className="configuration-diff-detail">
      <div className="configuration-diff-heading">
        <FileDiff size={15} />
        <strong>{draft.module}</strong>
        <code>{draft.id}</code>
        <Badge tone={draft.valid ? "success" : "danger"}>
          {draft.valid ? "校验通过" : "校验失败"}
        </Badge>
      </div>
      {!!draft.errors.length && (
        <ul className="configuration-diagnostics" aria-label="校验错误">
          {draft.errors.map((error, index) => (
            <li key={`${error.code}-${index}`}>
              <code>{error.code}</code>
              <span>{error.message}</span>
            </li>
          ))}
        </ul>
      )}
      {!!draft.risks.length && (
        <ul
          className="configuration-diagnostics configuration-risks"
          aria-label="草稿风险"
        >
          {draft.risks.map((risk, index) => (
            <li key={`${risk.code}-${index}`}>
              <code>{risk.code}</code>
              <span>{risk.message}</span>
            </li>
          ))}
        </ul>
      )}
      <pre
        className="configuration-diff"
        data-testid="configuration-diff"
        aria-label={`${draft.module} 草稿差异`}
      >
        {draft.diff
          ? draft.diff.split("\n").map((line, index) => (
              <span
                key={index}
                className={
                  line.startsWith("+") && !line.startsWith("+++")
                    ? "configuration-diff-add"
                    : line.startsWith("-") && !line.startsWith("---")
                      ? "configuration-diff-remove"
                      : line.startsWith("@@")
                        ? "configuration-diff-hunk"
                        : undefined
                }
              >
                {line || "\u00a0"}
              </span>
            ))
          : "无配置差异"}
      </pre>
    </div>
  );
}

export function DraftQueue({
  controller,
}: {
  controller: ConfigurationController;
}) {
  const [inspectedId, setInspectedId] = useState<string>();
  const inspected =
    controller.drafts.find((draft) => draft.id === inspectedId) ??
    controller.drafts.at(-1);
  return (
    <Panel className="configuration-draft-queue">
      <PanelHeader
        title="草稿队列"
        subtitle="暂存不应用；每个文档选择一个版本后 Commit。"
        action={<Badge>{controller.drafts.length} 个草稿</Badge>}
      />
      {controller.drafts.length ? (
        <>
          <div className="table-scroll">
            <table
              className="data-table configuration-drafts-table"
              aria-label="配置草稿"
            >
              <thead>
                <tr>
                  <th>选择</th>
                  <th>文档 / 草稿</th>
                  <th>版本</th>
                  <th>校验</th>
                  <th>风险</th>
                  <th className="align-right">操作</th>
                </tr>
              </thead>
              <tbody>
                {controller.drafts.map((draft) => {
                  const stale =
                    draft.generation !== controller.status?.generation;
                  const disabled =
                    !draft.valid ||
                    stale ||
                    !!controller.busy ||
                    !!controller.status?.pendingCommit;
                  return (
                    <tr
                      key={draft.id}
                      className={
                        inspected?.id === draft.id ? "row-selected" : undefined
                      }
                    >
                      <td>
                        <input
                          type="checkbox"
                          aria-label={`选择草稿 ${draft.id}`}
                          checked={controller.selectedIds.includes(draft.id)}
                          disabled={disabled}
                          onChange={(event) =>
                            controller.select(draft, event.target.checked)
                          }
                        />
                      </td>
                      <td>
                        <strong className="mono">{draft.module}</strong>
                        <small className="configuration-draft-id">
                          {draft.id}
                        </small>
                      </td>
                      <td>
                        <span className="mono">g{draft.generation}</span>
                        {stale && <Badge tone="warning">已过期</Badge>}
                      </td>
                      <td>
                        <Badge tone={draft.valid ? "success" : "danger"}>
                          {draft.valid
                            ? "通过"
                            : `${draft.errors.length} 个错误`}
                        </Badge>
                      </td>
                      <td>
                        {draft.risks.length ? (
                          <Badge tone="warning">
                            {draft.risks.length} 项风险
                          </Badge>
                        ) : (
                          <span className="text-muted">—</span>
                        )}
                      </td>
                      <td className="align-right">
                        <div className="configuration-actions">
                          <Button
                            variant="ghost"
                            size="small"
                            onClick={() => setInspectedId(draft.id)}
                            aria-label={`查看差异 ${draft.id}`}
                          >
                            差异
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            onClick={() => {
                              void controller.remove(draft.id);
                            }}
                            disabled={
                              !!controller.busy ||
                              !!controller.status?.pendingCommit
                            }
                            aria-label={`删除草稿 ${draft.id}`}
                          >
                            <Trash2 size={14} />
                          </Button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {inspected && <DraftDiff draft={inspected} />}
        </>
      ) : (
        <EmptyState
          icon={<FileDiff size={22} />}
          title="暂无草稿"
          detail="编辑配置并暂存，查看差异与校验结果。"
        />
      )}
    </Panel>
  );
}
