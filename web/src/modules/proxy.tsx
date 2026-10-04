import { useState } from "react";
import { Button, ErrorState } from "../components/ui/primitives";
import { api, errorMessage } from "../lib/api";
import { useResource } from "../lib/use-resource";
import type { ProxyNodes } from "../lib/contracts";
import { useRuntime } from "./runtime/use-runtime";
import { RuntimeControls } from "./runtime/controls";
import { NativeConfigEditor } from "./runtime/native-config-editor";
import { ProxyImportForm } from "./proxy/import-form";
import { NodeSelector } from "./proxy/node-selector";
import { GatewayPanel, type GatewaySetup } from "./proxy/gateway-panel";
import { CapturePanel } from "./proxy/capture-panel";
import { ConnectionAnalysis } from "./proxy/connection-analysis";
import { ProxyDiagnostics } from "./proxy/diagnostics";
import { ProxyPlanPreview } from "./proxy-plan-preview";
import { NetworkDiagnosticPanel } from "./proxy/network-diagnostic-panel";
import { LocalRulesEditor } from "./proxy/local-rules";

export function ProxyPage() {
  const runtime = useRuntime("sing-box");
  const resource = useResource(api.proxyNodes);
  const [imported, setImported] = useState<ProxyNodes>();
  const [importing, setImporting] = useState(false);
  const [capturing, setCapturing] = useState(false);
  const [rulesPending, setRulesPending] = useState(false);
  const [rulesVisited, setRulesVisited] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [refreshVersion, setRefreshVersion] = useState(0);
  const [tab, setTab] = useState("nodes");
  const nodes =
    imported || (resource.error === undefined ? resource.data : undefined);
  const busy = importing || runtime.pending || capturing || rulesPending;
  const guardedRuntime = { ...runtime, pending: busy };
  function openSetup(setup: GatewaySetup) {
    if (busy) return;
    if (setup === "node") setPickerOpen(true);
    else {
      setTab(setup === "runtime" ? "runtime" : "nodes");
      setAdvancedOpen(true);
    }
  }
  return (
    <div className="page-stack">
      <GatewayPanel
        runtime={runtime}
        nodes={nodes}
        nodesLoading={resource.loading}
        pending={importing || capturing || rulesPending}
        refreshVersion={refreshVersion}
        onPending={setCapturing}
        onSetup={openSetup}
      />
      <div className="form-actions" style={{ flexWrap: "wrap" }}>
        <span className="text-muted text-xs">
          精确域名直连、规则排序与订阅编辑
        </span>
        <Button
          type="button"
          size="small"
          disabled={busy}
          onClick={() => {
            setRulesVisited(true);
            setTab("rules");
            setAdvancedOpen(true);
          }}
        >
          编辑自定义规则
        </Button>
      </div>
      {resource.error !== undefined && (
        <ErrorState
          message={errorMessage(resource.error)}
          onRetry={busy ? undefined : resource.reload}
        />
      )}
      <details
        open={pickerOpen}
        onToggle={(event) => setPickerOpen(event.currentTarget.open)}
      >
        <summary>更换节点</summary>
        {pickerOpen && (
          <fieldset
            disabled={busy}
            style={{ border: 0, margin: 0, padding: 0, minWidth: 0 }}
          >
            <NodeSelector
              nodes={nodes}
              runtime={guardedRuntime}
              importing={importing || capturing}
              onSelected={() => {
                setImported(undefined);
                resource.reload();
              }}
            />
          </fieldset>
        )}
      </details>
      <details
        open={advancedOpen}
        onToggle={(event) => setAdvancedOpen(event.currentTarget.open)}
      >
        <summary>高级设置与诊断</summary>
        <div className="page-stack">
          <div className="page-toolbar">
            <div className="segmented" role="tablist" aria-label="代理视图">
              {[
                { id: "nodes", label: "节点" },
                { id: "runtime", label: "运行管理" },
                { id: "rules", label: "自定义规则" },
                { id: "capture", label: "设备诊断" },
                { id: "analysis", label: "连接分析" },
                { id: "diagnostics", label: "诊断" },
                { id: "requests", label: "网络诊断" },
                { id: "preview", label: "计划预览" },
              ].map((item) => (
                <button
                  role="tab"
                  disabled={busy}
                  key={item.id}
                  aria-selected={tab === item.id}
                  onClick={() => {
                    if (item.id === "rules") setRulesVisited(true);
                    setTab(item.id);
                  }}
                >
                  {item.label}
                </button>
              ))}
            </div>
            <Button
              size="small"
              disabled={resource.loading || busy}
              onClick={() => {
                setImported(undefined);
                resource.reload();
                runtime.refresh();
                setRefreshVersion((value) => value + 1);
              }}
            >
              刷新代理状态
            </Button>
          </div>
          <fieldset
            disabled={busy}
            style={{ border: 0, margin: 0, padding: 0, minWidth: 0 }}
          >
            {tab === "nodes" ? (
              <ProxyImportForm
                enabled={runtime.enabled && !busy}
                onImported={setImported}
                onPending={setImporting}
              />
            ) : tab === "runtime" ? (
              <>
                <RuntimeControls runtime={guardedRuntime} />
                <NativeConfigEditor runtime={guardedRuntime} />
              </>
            ) : tab === "capture" ? (
              <CapturePanel
                runtime={guardedRuntime}
                onPending={(pending) => {
                  setCapturing(pending);
                  if (!pending) setRefreshVersion((value) => value + 1);
                }}
              />
            ) : tab === "analysis" ? (
              <ConnectionAnalysis />
            ) : tab === "requests" ? (
              <NetworkDiagnosticPanel />
            ) : tab === "diagnostics" ? (
              <ProxyDiagnostics
                diagnostics={nodes?.diagnostics}
                policySummary={nodes?.policySummary}
              />
            ) : tab === "preview" ? (
              <ProxyPlanPreview />
            ) : null}
            {rulesVisited && (
              <div hidden={tab !== "rules"}>
                <LocalRulesEditor
                  runtime={runtime}
                  onPending={setRulesPending}
                />
              </div>
            )}
          </fieldset>
        </div>
      </details>
    </div>
  );
}
