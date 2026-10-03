import { useState } from "react";
import { Badge, Button, ErrorState } from "../components/ui/primitives";
import { api, errorMessage } from "../lib/api";
import { useResource } from "../lib/use-resource";
import type { ProxyNodes } from "../lib/contracts";
import { useRuntime } from "./runtime/use-runtime";
import { RuntimeControls } from "./runtime/controls";
import { NativeConfigEditor } from "./runtime/native-config-editor";
import { ProxyImportForm } from "./proxy/import-form";
import { NodeSelector } from "./proxy/node-selector";
import { CapturePanel } from "./proxy/capture-panel";
import { ConnectionAnalysis } from "./proxy/connection-analysis";
import { ProxyDiagnostics } from "./proxy/diagnostics";
import { ProxyPlanPreview } from "./proxy-plan-preview";
import { ProxySetupGuidance } from "./proxy/setup-guidance";

export function ProxyPage() {
  const runtime = useRuntime("sing-box");
  const resource = useResource(api.proxyNodes);
  const [imported, setImported] = useState<ProxyNodes>();
  const [importing, setImporting] = useState(false);
  const [capturing, setCapturing] = useState(false);
  const [tab, setTab] = useState("nodes");
  const nodes = imported || resource.data;
  return (
    <div className="page-stack">
      <div className="page-toolbar">
        <div className="segmented" role="tablist" aria-label="代理视图">
          {[
            { id: "nodes", label: "节点" },
            { id: "runtime", label: "运行管理" },
            { id: "capture", label: "客户端接管" },
            { id: "analysis", label: "连接分析" },
            { id: "diagnostics", label: "诊断" },
            { id: "preview", label: "计划预览" },
          ].map((item) => (
            <button
              role="tab"
              disabled={importing || runtime.pending || capturing}
              key={item.id}
              aria-selected={tab === item.id}
              onClick={() => setTab(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>
        <Badge
          tone={runtime.status?.state === "running" ? "success" : "neutral"}
        >
          sing-box · {runtime.status?.state ?? "读取中"}
        </Badge>
        <Button
          size="small"
          disabled={
            resource.loading || importing || runtime.pending || capturing
          }
          onClick={() => {
            setImported(undefined);
            resource.reload();
            runtime.refresh();
          }}
        >
          刷新代理状态
        </Button>
      </div>
      {resource.error !== undefined && (
        <ErrorState
          message={errorMessage(resource.error)}
          onRetry={resource.reload}
        />
      )}
      {tab === "nodes" ? (
        <>
          <ProxySetupGuidance
            runtime={runtime}
            nodes={nodes}
            busy={importing || runtime.pending || capturing}
            onRuntime={() => setTab("runtime")}
            onCapture={() => setTab("capture")}
          />
          <NodeSelector
            nodes={nodes}
            runtime={runtime}
            importing={importing}
            onSelected={() => {
              setImported(undefined);
              resource.reload();
            }}
          />
          <ProxyImportForm
            enabled={runtime.enabled && !runtime.pending}
            onImported={setImported}
            onPending={setImporting}
          />
        </>
      ) : tab === "runtime" ? (
        <>
          <RuntimeControls runtime={runtime} />
          <NativeConfigEditor runtime={runtime} />
        </>
      ) : tab === "capture" ? (
        <CapturePanel runtime={runtime} onPending={setCapturing} />
      ) : tab === "analysis" ? (
        <ConnectionAnalysis />
      ) : tab === "diagnostics" ? (
        <ProxyDiagnostics
          diagnostics={nodes?.diagnostics}
          policySummary={nodes?.policySummary}
        />
      ) : (
        <ProxyPlanPreview />
      )}
    </div>
  );
}
