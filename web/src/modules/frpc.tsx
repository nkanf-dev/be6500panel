import { useState } from "react";
import { Cable, FileCheck2, Plus, Trash2 } from "lucide-react";
import {
  Badge,
  Button,
  ErrorState,
  Field,
  Panel,
  PanelHeader,
  Select,
} from "../components/ui/primitives";
import { api, errorMessage, runRequest } from "../lib/api";
import { useResource } from "../lib/use-resource";
import {
  type FrpcPlanInput,
  type FrpcProxy,
  type OperationPlan,
} from "../lib/contracts";
import { PlanView } from "./plan-view";

const createProxy = (index: number): FrpcProxy => ({
  name: `service-${index}`,
  type: "tcp",
  localAddress: "127.0.0.1",
  localPort: 8080,
  remotePort: 18080,
});
const initial: FrpcPlanInput = {
  serverAddress: "",
  serverPort: 7000,
  tls: true,
  transport: "tcp",
  proxies: [createProxy(1)],
};
export function FrpcPage() {
  const status = useResource(api.frpc);
  const [input, setInput] = useState<FrpcPlanInput>(initial);
  const [plan, setPlan] = useState<OperationPlan>();
  const [plannedInput, setPlannedInput] = useState("");
  const [error, setError] = useState<unknown>();
  const [pending, setPending] = useState(false);
  const [nextId, setNextId] = useState(2);
  function update<K extends keyof FrpcPlanInput>(
    key: K,
    value: FrpcPlanInput[K],
  ) {
    setInput((previous) => ({ ...previous, [key]: value }));
  }
  function updateProxy(index: number, change: Partial<FrpcProxy>) {
    setInput((previous) => ({
      ...previous,
      proxies: previous.proxies.map((proxy, i) =>
        i === index ? { ...proxy, ...change } : proxy,
      ),
    }));
  }
  function changeType(index: number, type: FrpcProxy["type"]) {
    const proxy = input.proxies[index];
    const next: FrpcProxy = {
      name: proxy.name,
      type,
      localAddress: proxy.localAddress,
      localPort: proxy.localPort,
      ...(type === "tcp" || type === "udp"
        ? { remotePort: 18080 }
        : { domains: [] }),
    };
    update(
      "proxies",
      input.proxies.map((item, i) => (i === index ? next : item)),
    );
  }
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(undefined);
    try {
      const response = await runRequest(
        api.frpcPlan({
          ...input,
          proxies: input.proxies.map((proxy) =>
            proxy.domains
              ? { ...proxy, domains: proxy.domains.filter(Boolean) }
              : proxy,
          ),
        }),
      );
      setPlan(response);
      setPlannedInput(JSON.stringify(input));
    } catch (error) {
      setError(error);
    } finally {
      setPending(false);
    }
  }
  return (
    <div className="page-stack">
      <div className="page-toolbar">
        <span className="status-text text-muted">
          <Cable size={15} />
          frpc 进程{" "}
          <Badge tone={status.data?.running ? "success" : "neutral"}>
            {status.data?.running ? "运行中" : "未接入"}
          </Badge>
        </span>
        <span className="text-muted text-xs">
          映射 {input.proxies.length} / 64
        </span>
      </div>
      {status.error !== undefined && (
        <ErrorState
          message={errorMessage(status.error)}
          onRetry={status.reload}
        />
      )}
      <form onSubmit={submit}>
        <div className="planning-layout">
          <div className="page-stack">
            <Panel>
              <PanelHeader
                title="服务器连接"
                subtitle="连接与传输参数"
                action={<Cable size={16} />}
              />
              <div className="config-form">
                <Field label="服务器地址">
                  <input
                    placeholder="frps.example.com"
                    autoComplete="off"
                    required
                    value={input.serverAddress}
                    onChange={(event) =>
                      update("serverAddress", event.target.value.trim())
                    }
                  />
                </Field>
                <div className="form-grid">
                  <Field label="服务器端口">
                    <input
                      type="number"
                      min={1}
                      max={65535}
                      required
                      value={input.serverPort}
                      onChange={(event) =>
                        update("serverPort", event.target.valueAsNumber)
                      }
                    />
                  </Field>
                  <Field label="传输协议">
                    <Select
                      label="传输协议"
                      value={input.transport}
                      onValueChange={(value) =>
                        update("transport", value as FrpcPlanInput["transport"])
                      }
                      options={[
                        { value: "tcp", label: "TCP" },
                        { value: "quic", label: "QUIC" },
                      ]}
                    />
                  </Field>
                </div>
                <label className="checkbox-field">
                  <input
                    type="checkbox"
                    checked={input.tls}
                    onChange={(event) => update("tls", event.target.checked)}
                  />
                  <span>
                    <strong>启用 TLS</strong>
                    <small>加密客户端到服务器的连接</small>
                  </span>
                </label>
              </div>
            </Panel>
            <Panel>
              <PanelHeader
                title="服务映射"
                subtitle="TCP / UDP 端口或 HTTP(S) 域名"
                action={
                  <Button
                    type="button"
                    size="small"
                    disabled={input.proxies.length >= 64}
                    onClick={() => {
                      update("proxies", [
                        ...input.proxies,
                        createProxy(nextId),
                      ]);
                      setNextId((id) => id + 1);
                    }}
                  >
                    <Plus size={13} />
                    添加
                  </Button>
                }
              />
              <div className="proxy-editors">
                {input.proxies.map((proxy, index) => (
                  <section
                    key={index}
                    className="proxy-editor"
                    aria-label={`服务映射 ${index + 1}`}
                  >
                    <div className="proxy-editor-heading">
                      <Badge>{String(index + 1).padStart(2, "0")}</Badge>
                      <strong>{proxy.name || "未命名服务"}</strong>
                      <Button
                        variant="ghost"
                        size="icon"
                        type="button"
                        aria-label={`删除映射 ${index + 1}`}
                        onClick={() =>
                          update(
                            "proxies",
                            input.proxies.filter((_, i) => i !== index),
                          )
                        }
                      >
                        <Trash2 size={14} />
                      </Button>
                    </div>
                    <div className="form-grid">
                      <Field label="名称">
                        <input
                          required
                          maxLength={64}
                          value={proxy.name}
                          onChange={(event) =>
                            updateProxy(index, { name: event.target.value })
                          }
                        />
                      </Field>
                      <Field label="类型">
                        <Select
                          label={`映射 ${index + 1} 类型`}
                          value={proxy.type}
                          onValueChange={(value) =>
                            changeType(index, value as FrpcProxy["type"])
                          }
                          options={[
                            { value: "tcp", label: "TCP" },
                            { value: "udp", label: "UDP" },
                            { value: "http", label: "HTTP" },
                            { value: "https", label: "HTTPS" },
                          ]}
                        />
                      </Field>
                      <Field label="本地地址">
                        <input
                          required
                          value={proxy.localAddress}
                          onChange={(event) =>
                            updateProxy(index, {
                              localAddress: event.target.value,
                            })
                          }
                        />
                      </Field>
                      <Field label="本地端口">
                        <input
                          type="number"
                          min={1}
                          max={65535}
                          required
                          value={proxy.localPort}
                          onChange={(event) =>
                            updateProxy(index, {
                              localPort: event.target.valueAsNumber,
                            })
                          }
                        />
                      </Field>
                    </div>
                    {proxy.type === "tcp" || proxy.type === "udp" ? (
                      <Field label="远程端口">
                        <input
                          type="number"
                          min={1}
                          max={65535}
                          required
                          value={proxy.remotePort ?? ""}
                          onChange={(event) =>
                            updateProxy(index, {
                              remotePort: event.target.valueAsNumber,
                            })
                          }
                        />
                      </Field>
                    ) : (
                      <Field label="域名" hint="多个域名以逗号分隔">
                        <input
                          required
                          placeholder="app.example.com"
                          value={proxy.domains?.join(", ") ?? ""}
                          onChange={(event) =>
                            updateProxy(index, {
                              domains: event.target.value
                                .split(",")
                                .map((domain) => domain.trim()),
                            })
                          }
                        />
                      </Field>
                    )}
                  </section>
                ))}
                {!input.proxies.length && (
                  <div className="empty-inline">暂无服务映射</div>
                )}
              </div>
              <div className="config-form compact-form">
                {error !== undefined && (
                  <ErrorState message={errorMessage(error)} />
                )}
                <div className="form-actions">
                  <span className="text-muted text-xs">
                    POST /api/frpc/plan
                  </span>
                  <Button variant="primary" disabled={pending} type="submit">
                    <FileCheck2 size={15} />
                    {pending ? "校验中…" : "校验并生成计划"}
                  </Button>
                </div>
              </div>
            </Panel>
          </div>
          <PlanView
            plan={plan}
            stale={!!plan && plannedInput !== JSON.stringify(input)}
          />
        </div>
      </form>
    </div>
  );
}
