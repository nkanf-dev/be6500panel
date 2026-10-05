import { useState, useId, useEffect, type FormEvent } from "react";
import { AlertTriangle, CheckCircle2, ExternalLink, Loader2, Save } from "lucide-react";
import { Badge, Button, ErrorState } from "../../components/ui/primitives";
import { errorMessage, runRequest } from "../../lib/api";
import {
  featuresApi,
  formatReconnectUrl,
  pollOperation,
  type FeatureAction,
  type FeatureApplyResponse,
  type FeatureField,
  type FeatureImpact,
  type FeatureOperation,
  type FeatureState,
} from "../../lib/features-api";
import { FeatureFieldInput } from "./feature-field-input";

interface FeatureActionFormProps {
  domain: string;
  action: FeatureAction;
  state?: FeatureState;
  catalogGeneration?: number;
  targetPreFill?: Record<string, unknown>;
  onSuccess?: (response: FeatureApplyResponse) => void;
  onPending?: (pending: boolean) => void;
}

const impactLabels: Record<FeatureImpact, { label: string; tone: "neutral" | "warning" | "danger" }> = {
  local: { label: "即时生效", tone: "neutral" },
  network: { label: "网络重连", tone: "warning" },
  wireless: { label: "Wi-Fi 重启", tone: "warning" },
  maintenance: { label: "服务重启", tone: "danger" },
};

const impactConfirmMessages: Record<FeatureImpact, string> = {
  local: "确认应用此项修改？",
  network: "修改可能导致网络接口短暂断开或重新协商连接，请确认是否继续？",
  wireless: "修改将重启无线射频（Wi-Fi），当前已连接设备可能短暂重连，请确认是否继续？",
  maintenance: "此操作可能需要重启后台服务或相关设备，请确认是否继续？",
};

function extractFieldValue(
  domain: string,
  actionId: string,
  key: string,
  data?: Record<string, unknown>,
  preFill?: Record<string, unknown>,
): unknown {
  // 1. Explicit preFill priority (e.g. from selecting a table row or radio)
  if (preFill && key in preFill && preFill[key] !== undefined && preFill[key] !== "") {
    return preFill[key];
  }

  if (!data) return undefined;

  // If instance is explicitly unconfigured, do not pre-fill secrets
  if (
    data.configured === false &&
    (key.toLowerCase().includes("pwd") ||
      key.toLowerCase().includes("password") ||
      key.toLowerCase().includes("secret"))
  ) {
    return undefined;
  }

  // 2. Domain / Action scoped mappings based on actual factory controllers
  const info = (typeof data.info === "object" && data.info !== null ? data.info : {}) as Record<string, unknown>;
  const details = (typeof info.details === "object" && info.details !== null ? info.details : {}) as Record<string, unknown>;
  const ipv4List = Array.isArray(info.ipv4) ? (info.ipv4 as Array<Record<string, unknown>>) : [];
  const firstIpv4 = ipv4List[0] || {};

  // Action: set_lan_ip (domain: network)
  if (domain === "network" && actionId === "set_lan_ip") {
    if (key === "ip") return preFill?.ip ?? data.ip ?? firstIpv4.ip ?? info.ip;
    if (key === "mask") return preFill?.mask ?? data.mask ?? firstIpv4.mask ?? info.mask;
  }

  // Action: set_wan (domain: network)
  if (domain === "network" && actionId === "set_wan") {
    if (key === "wan_name") return preFill?.wan_name ?? data.wan_name ?? details.wan_name ?? "WAN1";
    if (key === "wanType") return preFill?.wanType ?? details.wanType ?? data.wanType;
    if (key === "mtu") return preFill?.mtu ?? details.mtu ?? info.mtu ?? data.mtu;
    if (key === "staticIp") return preFill?.staticIp ?? data.staticIp ?? firstIpv4.ip;
    if (key === "staticMask") return preFill?.staticMask ?? data.staticMask ?? firstIpv4.mask;
    if (key === "staticGateway") return preFill?.staticGateway ?? data.staticGateway ?? info.gateWay ?? info.gateway;
    if (key === "dns1") return preFill?.dns1 ?? info.dnsAddrs ?? info.dnsAddrs1 ?? data.dns1;
    if (key === "dns2") return preFill?.dns2 ?? info.dnsAddrs2 ?? data.dns2;
  }

  // Action: set_wifi (domain: wireless)
  if (domain === "wireless" && actionId === "set_wifi") {
    if (preFill && key in preFill) return preFill[key];
    const wifiInfoList = Array.isArray(info) ? (info as Array<Record<string, unknown>>) : [];
    const firstRadio = wifiInfoList[0] || {};
    if (key in firstRadio) return firstRadio[key];
  }

  // Action: set_guest_wifi (domain: wireless)
  if (domain === "wireless" && actionId === "set_guest_wifi") {
    if (key === "wifiIndex") return preFill?.wifiIndex ?? data.wifiIndex ?? 3;
    if (key === "ssid") return preFill?.ssid ?? data.ssid ?? info.ssid;
    if (key === "on") return preFill?.on ?? data.on ?? info.on;
    if (key === "encryption") return preFill?.encryption ?? data.encryption ?? info.encryption;
  }

  // Action: led_set / eth_led_set / all_led_set (domain: services)
  if (domain === "services" && (actionId === "led_set" || actionId === "eth_led_set" || actionId === "all_led_set")) {
    if (key === "on") return preFill?.on ?? data.status ?? data.on ?? info.status ?? info.on;
    if (key === "timer_on") return preFill?.timer_on ?? data.timer_status ?? data.timer_on ?? info.timer_status ?? info.timer_on ?? 0;
    if (key === "timer_open") return preFill?.timer_open ?? data.timer_open ?? data.time_open ?? info.timer_open ?? info.time_open ?? "00:00";
    if (key === "timer_close") return preFill?.timer_close ?? data.timer_close ?? data.time_close ?? info.timer_close ?? info.time_close ?? "00:00";
  }

  // Action: upnp_switch (domain: services)
  if (domain === "services" && actionId === "upnp_switch") {
    if (key === "switch") return preFill?.switch ?? data.status ?? data.switch;
  }

  // Action: scheduled_reboot_set (domain: services) - default enabled is false
  if (domain === "services" && actionId === "scheduled_reboot_set") {
    if (key === "enabled") return preFill?.enabled ?? data.enabled ?? false;
    if (key === "time") return preFill?.time ?? data.time ?? "03:00";
    if (key === "weekdays") return preFill?.weekdays ?? data.weekdays;
  }

  // Action: forward_add (domain: services) - actual factory DTO rows use destip, srcport, destport, proto
  if (domain === "services" && actionId === "forward_add" && preFill) {
    if (key === "name") return preFill.name;
    if (key === "ip") return preFill.ip ?? preFill.destip;
    if (key === "sport") return preFill.sport ?? preFill.srcport;
    if (key === "dport") return preFill.dport ?? preFill.destport;
    if (key === "proto") return preFill.proto;
  }

  // 3. Fallback: exact match in data or direct sub-object (no uncontrolled recursive DFS)
  const wan6_cfg = (typeof data.wan6_cfg === "object" && data.wan6_cfg !== null ? data.wan6_cfg : {}) as Record<string, unknown>;
  if (key in data) return data[key];
  if (key in info) return info[key];
  if (key in details) return details[key];
  if (key in wan6_cfg) return wan6_cfg[key];

  return undefined;
}

export function FeatureActionForm({
  domain,
  action,
  state,
  catalogGeneration,
  targetPreFill,
  onSuccess,
  onPending,
}: FeatureActionFormProps) {
  const formId = useId();

  const getInitialValues = () => {
    const init: Record<string, unknown> = {};
    for (const field of action.fields) {
      const val = extractFieldValue(domain, action.id, field.key, state?.data, targetPreFill);
      if (val !== undefined) {
        init[field.key] = val;
      }
    }
    return init;
  };

  const [values, setValues] = useState<Record<string, unknown>>(getInitialValues);
  const [dirty, setDirty] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [pendingOpId, setPendingOpId] = useState<string>();
  const [activeOperation, setActiveOperation] = useState<FeatureOperation>();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [error, setError] = useState<unknown>();
  const [successMessage, setSuccessMessage] = useState<string>();

  // Update values if targetPreFill changes
  useEffect(() => {
    if (targetPreFill) {
      setValues((prev) => {
        const next = { ...prev };
        for (const field of action.fields) {
          const projected=extractFieldValue(domain,action.id,field.key,state?.data,targetPreFill);
          if(projected!==undefined){next[field.key]=projected;}
        }
        return next;
      });
      setDirty(true);
    }
  }, [targetPreFill,action.fields,action.id,domain,state?.data]);

  const effectiveGeneration = state?.generation ?? catalogGeneration;

  const isConfigured = (key: string) => {
    if (state?.data?.configured === false) {
      return false;
    }

    if (action.id === "set_wan") {
      const info = (typeof state?.data?.info === "object" && state?.data?.info !== null ? state?.data?.info : {}) as Record<string, unknown>;
      const details = (typeof info.details === "object" && info.details !== null ? info.details : {}) as Record<string, unknown>;

      if (key === "pppoeName") {
        return Boolean(
          state?.data?.pppoeNameConfigured ||
          details.usernameConfigured ||
          details.pppoeNameConfigured ||
          state?.data?.usernameConfigured
        );
      }
      if (key === "pppoePwd") {
        return Boolean(
          state?.data?.pppoePwdConfigured ||
          details.passwordConfigured ||
          details.pppoePwdConfigured ||
          state?.data?.passwordConfigured
        );
      }
    }

    if (action.id === "set_wifi") {
      if (key === "pwd") {
        return Boolean(
          targetPreFill?.pwdConfigured ||
          targetPreFill?.passwordConfigured ||
          state?.data?.pwdConfigured ||
          state?.data?.passwordConfigured
        );
      }
    }

    if (action.id === "set_guest_wifi") {
      if (key === "pwd") {
        const info = (typeof state?.data?.info === "object" && state?.data?.info !== null ? state?.data?.info : {}) as Record<string, unknown>;
        return Boolean(
          state?.data?.pwdConfigured ||
          state?.data?.passwordConfigured ||
          info.pwdConfigured ||
          info.passwordConfigured
        );
      }
    }

    return Boolean(
      state?.data?.[`${key}Configured`] ||
      targetPreFill?.[`${key}Configured`]
    );
  };

  const handleFieldChange = (key: string, value: unknown) => {
    setValues((prev) => ({ ...prev, [key]: value }));
    setDirty(true);
    setSuccessMessage(undefined);
    setError(undefined);
  };

  const executeApply = async (acknowledgeImpact = false) => {
    setSubmitting(true);
    onPending?.(true);
    setError(undefined);
    setSuccessMessage(undefined);

    try {
      // Build filtered input: omit empty secrets to preserve existing backend secrets
      const inputPayload: Record<string, unknown> = {};
      for (const field of action.fields) {
        const val = values[field.key];
        if (field.kind === "secret" && (val === undefined || val === "")) {
          // Omit to preserve existing secret
          continue;
        }
        if (val !== undefined) {
          inputPayload[field.key] = val;
        }
      }

      const response = await runRequest(
        featuresApi.apply(domain, {
          actionId: action.id,
          input: inputPayload,
          generation: effectiveGeneration,
          acknowledgeImpact: acknowledgeImpact || action.impact === "local",
        }),
      );

      // Immediately show reconnectAddress upon receiving pending operation
      if (response.operation.reconnectAddress || response.operation.canConfirm) {
        setActiveOperation(response.operation);
      }

      if (response.operation.canConfirm) {
        setActiveOperation(response.operation);
        setDirty(false);
      } else if (response.operation.state === "pending") {
        setPendingOpId(response.operation.id);
        const finalOp = await pollOperation(response.operation.id, {
          initialOperation: response.operation,
          onProgress: (op) => {
            if (op.reconnectAddress || op.canConfirm) {
              setActiveOperation(op);
            }
          },
        });
        setPendingOpId(undefined);
        if (finalOp.canConfirm || finalOp.reconnectAddress) {
          setActiveOperation(finalOp);
          setDirty(false);
        } else if (finalOp.state === "completed") {
          setActiveOperation(undefined);
          setSuccessMessage("配置已成功应用并生效");
          setDirty(false);
          onSuccess?.(response);
        } else if (finalOp.state === "pending") {
          setActiveOperation(finalOp);
          setSuccessMessage("配置已在后台处理中，请稍后刷新状态核对。");
          setDirty(false);
          onSuccess?.(response);
        }
      } else if (response.operation.state === "completed") {
        setActiveOperation(undefined);
        setSuccessMessage("配置已成功应用并生效");
        setDirty(false);
        onSuccess?.(response);
      } else {
        throw new Error(response.operation.error || "操作未成功");
      }
    } catch (cause) {
      setError(cause);
    } finally {
      setSubmitting(false);
      onPending?.(false);
      setConfirmOpen(false);
    }
  };

  const handleConfirmConnection = async () => {
    if (!activeOperation) return;
    setConfirming(true);
    setError(undefined);
    try {
      const res = await runRequest(featuresApi.confirm(activeOperation.id));
      if (res.state === "completed") {
        setActiveOperation(undefined);
        setSuccessMessage("连接正常，配置已确认生效");
        onSuccess?.({ operation: res });
      } else if (res.state === "failed") {
        throw new Error(res.error || "配置确认未成功");
      }
    } catch (cause) {
      setError(cause);
    } finally {
      setConfirming(false);
    }
  };

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (submitting) return;

    if (action.impact !== "local") {
      setConfirmOpen(true);
    } else {
      void executeApply(false);
    }
  };

  const impactMeta = impactLabels[action.impact] ?? {
    label: action.impact,
    tone: "neutral" as const,
  };

  return (
    <form id={formId} onSubmit={handleSubmit} className="config-form page-stack border border-border/80 rounded-lg p-4 bg-surface/50">
      <div className="flex items-center justify-between gap-2 border-b border-border pb-2">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-semibold">{action.title}</h3>
          <Badge tone={impactMeta.tone}>{impactMeta.label}</Badge>
        </div>
        {effectiveGeneration !== undefined && (
          <span className="text-xs font-mono text-muted">
            代次: #{effectiveGeneration}
          </span>
        )}
      </div>

      {error !== undefined && (
        <ErrorState
          message={errorMessage(error)}
          onRetry={() => void executeApply(true)}
        />
      )}

      {successMessage && (
        <div className="flex items-center gap-2 text-xs text-primary bg-primary/10 rounded px-3 py-2">
          <CheckCircle2 size={14} />
          <span>{successMessage}</span>
        </div>
      )}

      <div className="form-grid gap-4">
        {action.fields.map((field: FeatureField) => (
          <FeatureFieldInput
            key={field.key}
            field={field}
            value={values[field.key]}
            configured={isConfigured(field.key)}
            disabled={submitting}
            onChange={(val) => handleFieldChange(field.key, val)}
          />
        ))}
      </div>

      {activeOperation && (activeOperation.canConfirm || activeOperation.reconnectAddress) && (
        <div className="p-4 rounded-lg border border-warning bg-warning/10 space-y-3" role="region" aria-label="连接确认">
          <div className="flex items-start gap-2">
            <AlertTriangle className="text-warning mt-0.5 shrink-0" size={16} />
            <div className="text-xs space-y-1">
              <p className="font-semibold text-foreground">
                {activeOperation.canConfirm ? "配置已应用，等待连接确认" : "配置正在应用中…"}
              </p>
              {activeOperation.waitingFor && (
                <p className="text-muted">{activeOperation.waitingFor}</p>
              )}
              {activeOperation.reconnectAddress && (
                <p className="text-foreground pt-0.5">
                  新管理地址：
                  <a
                    href={formatReconnectUrl(activeOperation.reconnectAddress)}
                    target="_blank"
                    rel="noreferrer"
                    className="font-mono font-medium underline text-primary hover:text-primary/80 inline-flex items-center gap-1"
                  >
                    {formatReconnectUrl(activeOperation.reconnectAddress) || activeOperation.reconnectAddress}
                    <ExternalLink size={11} className="inline" />
                  </a>
                </p>
              )}
            </div>
          </div>
          {activeOperation.canConfirm && (
            <div className="flex justify-end gap-2">
              <Button
                type="button"
                variant="primary"
                size="small"
                disabled={confirming}
                onClick={handleConfirmConnection}
              >
                {confirming ? "正在确认…" : "确认连接正常"}
              </Button>
            </div>
          )}
        </div>
      )}

      <div className="form-actions justify-end pt-2">
        {effectiveGeneration === undefined && (
          <span className="text-xs text-muted mr-auto">正在读取配置版本…</span>
        )}
        <Button
          type="submit"
          variant="primary"
          disabled={submitting || confirming || !!activeOperation?.canConfirm || effectiveGeneration === undefined || (!dirty && !action.fields.some((f) => f.required))}
        >
          {submitting ? (
            <>
              <Loader2 size={14} className="animate-spin mr-1 inline" />
              {pendingOpId ? "正在等待生效…" : "正在保存…"}
            </>
          ) : (
            <>
              <Save size={14} className="mr-1 inline" />
              应用设置
            </>
          )}
        </Button>
      </div>

      {confirmOpen && (
        <div
          role="dialog"
          aria-modal="true"
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
        >
          <div className="bg-background border border-border rounded-lg max-w-md w-full p-5 space-y-4 shadow-xl">
            <div className="flex items-start gap-3">
              <div className="p-2 rounded-full bg-warning/10 text-warning">
                <AlertTriangle size={20} />
              </div>
              <div className="space-y-1">
                <h4 className="font-semibold text-base">确认执行操作？</h4>
                <p className="text-xs text-muted leading-relaxed">
                  {impactConfirmMessages[action.impact] ?? "确认应用当前修改？"}
                </p>
              </div>
            </div>
            <div className="form-actions justify-end gap-2 pt-2">
              <Button
                type="button"
                variant="ghost"
                disabled={submitting}
                onClick={() => setConfirmOpen(false)}
              >
                取消
              </Button>
              <Button
                type="button"
                variant="primary"
                disabled={submitting}
                onClick={() => void executeApply(true)}
              >
                {submitting ? "正在执行…" : "确认继续"}
              </Button>
            </div>
          </div>
        </div>
      )}
    </form>
  );
}
