import { useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import {
  Badge,
  Button,
  ErrorState,
  Field,
} from "../../components/ui/primitives";
import { ApiError, errorMessage } from "../../lib/api";
import { useDeviceLabels } from "./device-labels";
import type { WorkspaceDevice } from "./device-model";

const length = (value: string) => Array.from(value).length;
export function EditDeviceAnnotation({
  device,
  onClose,
}: {
  device: WorkspaceDevice;
  onClose: () => void;
}) {
  const labels = useDeviceLabels();
  const original = labels.annotations[device.mac];
  const [label, setLabel] = useState(original?.label ?? "");
  const [note, setNote] = useState(original?.note ?? "");
  const [tagText, setTagText] = useState(original?.tags.join(", ") ?? "");
  const [revision, setRevision] = useState(labels.revision);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<unknown>();
  const tags = [
    ...new Set(
      tagText
        .split(/[,，]/)
        .map((tag) => tag.trim())
        .filter(Boolean),
    ),
  ];
  const validation =
    length(label) > 80
      ? "备注名称最多 80 个字符"
      : length(note) > 1000
        ? "详细备注最多 1000 个字符"
        : tags.length > 8
          ? "最多添加 8 个标签"
          : tags.some((tag) => length(tag) > 32)
            ? "每个标签最多 32 个字符"
            : undefined;
  const reloadLatest = () => {
    const latest = labels.annotations[device.mac];
    setLabel(latest?.label ?? "");
    setNote(latest?.note ?? "");
    setTagText(latest?.tags.join(", ") ?? "");
    setRevision(labels.revision);
    setError(undefined);
  };
  const save = async () => {
    if (saving || validation || labels.loading || labels.error) return;
    setSaving(true);
    setError(undefined);
    try {
      await labels.save({
        mac: device.mac,
        label,
        note,
        tags,
        expectedRevision: revision,
      });
      onClose();
    } catch (cause) {
      setError(cause);
      if (cause instanceof ApiError && cause.status === 409) labels.refresh();
    } finally {
      setSaving(false);
    }
  };
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open && !saving) onClose();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="device-dialog-overlay" />
        <Dialog.Content
          className="device-dialog"
          onEscapeKeyDown={(event) => {
            if (saving) event.preventDefault();
          }}
          onPointerDownOutside={(event) => event.preventDefault()}
        >
          <div className="device-dialog-heading">
            <div>
              <Dialog.Title>编辑设备备注</Dialog.Title>
              <Dialog.Description>
                名称、备注和标签保存在此网关，保存后同步更新控制中心的设备显示。
              </Dialog.Description>
            </div>
            <Dialog.Close asChild>
              <Button
                variant="ghost"
                size="icon"
                disabled={saving}
                aria-label="关闭备注编辑"
              >
                <X size={18} />
              </Button>
            </Dialog.Close>
          </div>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void save();
            }}
            className="device-edit-form"
          >
            <Field
              label="设备备注名称"
              hint="清空名称后显示系统主机名或 MAC 地址。"
            >
              <input
                aria-label="设备备注名称"
                autoFocus
                value={label}
                onChange={(event) => setLabel(event.target.value)}
                placeholder="我的电脑、客厅电视…"
                disabled={saving}
              />
            </Field>
            <Field
              label="详细备注"
              hint={`${length(note)} / 1000 字符 · 供本局域网管理员使用，按明文保存。`}
            >
              <textarea
                aria-label="详细备注"
                rows={4}
                value={note}
                onChange={(event) => setNote(event.target.value)}
                placeholder="设备位置、用途和维护记录…"
                disabled={saving}
              />
            </Field>
            <Field
              label="设备标签"
              hint="用逗号分隔；最多 8 个，每个最多 32 个字符。"
            >
              <input
                aria-label="设备标签"
                value={tagText}
                onChange={(event) => setTagText(event.target.value)}
                placeholder="办公, 常用设备"
                disabled={saving}
              />
            </Field>
            <details className="device-hardware-details">
              <summary>系统硬件详情</summary>
              <dl className="key-values">
                <div>
                  <dt>系统原始名称</dt>
                  <dd>{device.hostname || "未提供"}</dd>
                </div>
                <div>
                  <dt>MAC 地址</dt>
                  <dd className="mono">{device.mac}</dd>
                </div>
                <div>
                  <dt>设备地址</dt>
                  <dd className="mono">
                    {device.addresses.join(" · ") || "未提供"}
                  </dd>
                </div>
                <div>
                  <dt>接口</dt>
                  <dd>{device.activity?.interface || "未提供"}</dd>
                </div>
                {device.activity?.vendor && (
                  <div>
                    <dt>厂商</dt>
                    <dd>{device.activity.vendor}</dd>
                  </div>
                )}
              </dl>
            </details>
            {validation && (
              <p role="alert" className="text-danger">
                {validation}
              </p>
            )}
            {labels.error !== undefined && (
              <ErrorState
                message={`读取备注失败：${errorMessage(labels.error)}`}
                onRetry={labels.refresh}
              />
            )}
            {error !== undefined && (
              <ErrorState message={errorMessage(error)} />
            )}
            {error instanceof ApiError && error.status === 409 && (
              <div className="device-conflict">
                <Badge tone="warning">备注已被另一会话更新</Badge>
                <p>当前输入仍保留。载入最新备注后检查并重新保存。</p>
                <Button
                  type="button"
                  size="small"
                  disabled={labels.loading || labels.error !== undefined}
                  onClick={reloadLatest}
                >
                  载入最新备注
                </Button>
              </div>
            )}
            <div className="device-dialog-actions">
              <Button type="button" onClick={onClose} disabled={saving}>
                取消
              </Button>
              <Button
                type="submit"
                variant="primary"
                disabled={
                  saving ||
                  !!validation ||
                  labels.loading ||
                  labels.error !== undefined
                }
              >
                {saving ? "正在保存备注…" : "保存备注"}
              </Button>
            </div>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
