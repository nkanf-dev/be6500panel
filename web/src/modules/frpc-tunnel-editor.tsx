import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import {
  Badge,
  Button,
  Field,
  Panel,
  PanelHeader,
  Select,
} from "../components/ui/primitives";
import type { FrpcProxy } from "../lib/contracts";

export const createFrpcProxy = (index: number): FrpcProxy => ({
  name: `service-${index}`,
  type: "tcp",
  localAddress: "127.0.0.1",
  localPort: 8080,
  remotePort: 18080,
});

export function FrpcTunnelEditor({
  proxies,
  onChange,
  disabled = false,
  isSavedMapping = () => false,
}: {
  proxies: FrpcProxy[];
  onChange: (proxies: FrpcProxy[]) => void;
  disabled?: boolean;
  isSavedMapping?: (proxy: FrpcProxy) => boolean;
}) {
  const [removeIndex, setRemoveIndex] = useState<number>();
  const [nextId, setNextId] = useState(2);
  function update(index: number, change: Partial<FrpcProxy>) {
    onChange(
      proxies.map((proxy, i) =>
        i === index ? { ...proxy, ...change } : proxy,
      ),
    );
  }
  function changeType(index: number, type: FrpcProxy["type"]) {
    const proxy = proxies[index];
    onChange(
      proxies.map((item, i) =>
        i !== index
          ? item
          : {
              ...proxy,
              name: proxy.name,
              type,
              localAddress: proxy.localAddress,
              localPort: proxy.localPort,
              ...(type === "tcp" || type === "udp"
                ? { remotePort: 18080 }
                : { domains: [] }),
            },
      ),
    );
  }
  function add() {
    let id = nextId;
    while (proxies.some((proxy) => proxy.name === `service-${id}`)) id += 1;
    onChange([...proxies, createFrpcProxy(id)]);
    setNextId(id + 1);
  }
  return (
    <Panel>
      <PanelHeader
        title="服务映射"
        subtitle="TCP / UDP 端口或 HTTP(S) 域名"
        action={
          <Button
            type="button"
            size="small"
            disabled={disabled || proxies.length >= 64}
            onClick={add}
          >
            <Plus size={13} />
            添加
          </Button>
        }
      />
      <div className="proxy-editors">
        {proxies.map((proxy, index) => (
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
                disabled={disabled}
                aria-label={`删除映射 ${index + 1}`}
                onClick={() => {
                  if (isSavedMapping(proxy)) setRemoveIndex(index);
                  else onChange(proxies.filter((_, i) => i !== index));
                }}
              >
                <Trash2 size={14} />
              </Button>
            </div>
            <div className="form-grid">
              <Field label="名称">
                <input
                  required
                  disabled={disabled}
                  maxLength={64}
                  value={proxy.name}
                  onChange={(event) =>
                    update(index, { name: event.target.value })
                  }
                />
              </Field>
              <Field label="类型">
                <Select
                  label={`映射 ${index + 1} 类型`}
                  value={proxy.type}
                  onValueChange={(value) => {
                    if (!disabled)
                      changeType(index, value as FrpcProxy["type"]);
                  }}
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
                  disabled={disabled}
                  autoComplete="off"
                  value={proxy.localAddress}
                  onChange={(event) =>
                    update(index, { localAddress: event.target.value })
                  }
                />
              </Field>
              <Field label="本地端口">
                <input
                  type="number"
                  min={0}
                  max={65535}
                  required
                  disabled={disabled}
                  value={Number.isNaN(proxy.localPort) ? "" : proxy.localPort}
                  onChange={(event) =>
                    update(index, { localPort: event.target.valueAsNumber })
                  }
                />
              </Field>
            </div>
            {proxy.type === "tcp" || proxy.type === "udp" ? (
              <Field label="远程端口">
                <input
                  type="number"
                  min={0}
                  max={65535}
                  required
                  disabled={disabled}
                  value={
                    Number.isNaN(proxy.remotePort)
                      ? ""
                      : (proxy.remotePort ?? "")
                  }
                  onChange={(event) =>
                    update(index, { remotePort: event.target.valueAsNumber })
                  }
                />
              </Field>
            ) : (
              <Field label="域名" hint="多个域名以逗号分隔">
                <input
                  required
                  disabled={disabled}
                  placeholder="app.example.com"
                  autoComplete="off"
                  value={proxy.domains?.join(", ") ?? ""}
                  onChange={(event) =>
                    update(index, {
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
        {removeIndex !== undefined && proxies[removeIndex] && (
          <div role="group" aria-label="删除映射确认" className="config-form">
            <p>
              删除映射“{proxies[removeIndex].name}
              ”？保存后会同时移除该映射的原生与扩展参数。
            </p>
            <div>
              <Button type="button" onClick={() => setRemoveIndex(undefined)}>
                保留映射
              </Button>{" "}
              <Button
                type="button"
                disabled={disabled}
                onClick={() => {
                  onChange(proxies.filter((_, index) => index !== removeIndex));
                  setRemoveIndex(undefined);
                }}
              >
                确认删除映射
              </Button>
            </div>
          </div>
        )}
        {!proxies.length && <div className="empty-inline">暂无服务映射</div>}
      </div>
    </Panel>
  );
}
