import { strings } from "../../locales/strings";
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
import {
  configurationModules,
  type ConfigurationDraft,
  type ConfigurationModule,
} from "./contracts";

function moduleLabel(module: ConfigurationModule) {
  return configurationModules.find((item) => item.module === module)!.label;
}

export function DraftDiff({ draft }: { draft: ConfigurationDraft }) {
  const label = moduleLabel(draft.module);
  return (
    <div className="configuration-diff-detail">
      <div className="configuration-diff-heading">
        <FileDiff size={15} />
        <strong>{label}更改详情</strong>
        <Badge
          tone={
            !draft.valid
              ? "danger"
              : draft.dependencies?.length
                ? "warning"
                : "success"
          }
        >
          {!draft.valid
            ? "检查未通过"
            : draft.dependencies?.length
              ? "引用待完整检查"
              : "检查通过"}
        </Badge>
      </div>
      {!!draft.dependencies?.length && (
        <div role="status" className="configuration-inline-warning">
          此草稿引用了尚未应用的配置。请一并选择所需草稿；应用前会检查完整配置组合。
        </div>
      )}
      {!!draft.errors.length && (
        <ul className="configuration-diagnostics" aria-label="检查问题">
          {draft.errors.map((error, index) => (
            <li key={`${error.code}-${index}`}>
              <span>{error.message}</span>
            </li>
          ))}
        </ul>
      )}
      {!!draft.risks.length && (
        <ul
          className="configuration-diagnostics configuration-risks"
          aria-label="更改风险"
        >
          {draft.risks.map((risk, index) => (
            <li key={`${risk.code}-${index}`}>
              <span>{risk.message}</span>
            </li>
          ))}
        </ul>
      )}
      <details className="configuration-advanced-details">
        <summary>{strings.configuration.advancedDetails}</summary>
        <p>
          检查标识：<code>{draft.id}</code>
        </p>
        <p>
          配置版本：<code>{draft.generation}</code>
        </p>
        {(draft.errors.length > 0 || draft.risks.length > 0) && (
          <ul className="configuration-diagnostics">
            {draft.errors.map((error, index) => (
              <li key={`error-${index}`}>
                问题代码：<code>{error.code}</code>
              </li>
            ))}
            {draft.risks.map((risk, index) => (
              <li key={`risk-${index}`}>
                风险代码：<code>{risk.code}</code>
              </li>
            ))}
          </ul>
        )}
        <h3>原生配置差异</h3>
        <pre
          className="configuration-diff"
          data-testid="configuration-diff"
          aria-label={`${label}原生配置差异`}
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
      </details>
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
        title={strings.configuration.queueTitle}
        subtitle={strings.configuration.queueSubtitle}
        action={<Badge>{controller.drafts.length} 个检查结果</Badge>}
      />
      {controller.drafts.length ? (
        <>
          <div className="table-scroll">
            <table
              className="data-table configuration-drafts-table"
              aria-label="配置检查结果"
            >
              <thead>
                <tr>
                  <th>{strings.proxy.nodes.columns.select}</th>
                  <th>配置文档 / 草稿</th>
                  <th>适用状态</th>
                  <th>检查</th>
                  <th>风险</th>
                  <th className="align-right">操作</th>
                </tr>
              </thead>
              <tbody>
                {controller.drafts.map((draft) => {
                  const label = moduleLabel(draft.module);
                  const version =
                    controller.drafts
                      .filter((item) => item.module === draft.module)
                      .findIndex((item) => item.id === draft.id) + 1;
                  const draftLabel = `${label}草稿 ${version}`;
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
                          aria-label={`选择${draftLabel}`}
                          checked={controller.selectedIds.includes(draft.id)}
                          disabled={disabled}
                          onChange={(event) =>
                            controller.select(draft, event.target.checked)
                          }
                        />
                      </td>
                      <td>
                        <strong>{label}</strong>
                        <small className="configuration-draft-id">
                          {strings.states.draft} {version}
                        </small>
                      </td>
                      <td>
                        {stale ? (
                          <Badge tone="warning">需要重新检查</Badge>
                        ) : (
                          <span className="text-muted">基于当前配置</span>
                        )}
                      </td>
                      <td>
                        <Badge
                          tone={
                            !draft.valid
                              ? "danger"
                              : draft.dependencies?.length
                                ? "warning"
                                : "success"
                          }
                        >
                          {!draft.valid
                            ? `${draft.errors.length} 个问题`
                            : draft.dependencies?.length
                              ? "引用待完整检查"
                              : "通过"}
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
                            aria-label={`查看${draftLabel}详情`}
                          >
                            {strings.actions.viewDetails}
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
                            aria-label={`删除${draftLabel}`}
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
          title={strings.configuration.emptyQueueTitle}
          detail={strings.configuration.emptyQueueDetail}
        />
      )}
    </Panel>
  );
}
