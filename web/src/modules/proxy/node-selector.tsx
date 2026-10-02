import { Effect } from "effect";
import { useState } from "react";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Field,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { api, errorMessage } from "../../lib/api";
import type {
  IPv6Policy,
  ProxyNodes,
  ProxySelectInput,
} from "../../lib/contracts";
import type { RuntimeController } from "../runtime/use-runtime";

export function NodeSelector({
  nodes,
  runtime,
  onSelected,
  importing = false,
}: {
  nodes?: ProxyNodes;
  importing?: boolean;
  runtime: RuntimeController;
  onSelected: () => void;
}) {
  const [nodeId, setNodeId] = useState("");
  const [ipv6, setIPv6] = useState<IPv6Policy>("direct");
  const [ports, setPorts] = useState<ProxySelectInput["ports"]>({
    mixed: 2080,
    tproxy: 7893,
    dns: 6450,
  });
  const [pending, setPending] = useState(false);
  const [sha256, setSHA256] = useState<string>();
  const selectedId = nodeId || nodes?.selectedNodeId || "";
  const valid = nodes?.nodes.some((node) => node.id === selectedId);
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (
      importing ||
      pending ||
      runtime.pending ||
      !runtime.enabled ||
      !valid ||
      !runtime.status?.artifactAvailable
    )
      return;
    setPending(true);
    setSHA256(undefined);
    try {
      await runtime.run(
        () =>
          api
            .proxySelect({ nodeId: selectedId, ipv6, failure: "direct", ports })
            .pipe(
              Effect.map((response) => {
                setSHA256(response.configSHA256);
                onSelected();
                return response.status;
              }),
            ),
        "节点配置已 Commit",
      );
    } finally {
      setPending(false);
    }
  }
  return (
    <Panel>
      <PanelHeader
        title="代理节点"
        subtitle="选择真实节点，编译并验证 sing-box 原生配置"
        action={<Badge>{nodes?.nodes.length ?? 0} 个节点</Badge>}
      />
      <form onSubmit={submit}>
        <div className="table-scroll">
          <table className="data-table">
            <thead>
              <tr>
                <th>选择</th>
                <th>名称 / 出口</th>
                <th>协议</th>
                <th>能力</th>
              </tr>
            </thead>
            <tbody>
              {nodes?.nodes.map((node) => (
                <tr key={node.id}>
                  <td>
                    <input
                      disabled={runtime.pending || pending || importing}
                      type="radio"
                      name="proxy-node"
                      aria-label={`选择节点 ${node.label}`}
                      checked={selectedId === node.id}
                      onChange={() => {
                        setNodeId(node.id);
                        setSHA256(undefined);
                      }}
                    />
                  </td>
                  <td>
                    <strong>{node.label}</strong>
                    <div className="mono text-muted">
                      {node.server}:{node.port}
                    </div>
                  </td>
                  <td>
                    {node.protocol.toUpperCase()} /{" "}
                    {node.transport.toUpperCase()}
                  </td>
                  <td>
                    {[
                      node.reality && "REALITY",
                      node.vision && "Vision",
                      node.utls && "uTLS",
                      node.udp && "UDP",
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {!nodes?.nodes.length && (
          <EmptyState title="暂无节点" detail="先导入订阅" />
        )}
        <div className="config-form">
          <div className="form-grid">
            <Field label="节点 IPv6 策略">
              <select
                className="select-trigger"
                disabled={runtime.pending || pending || importing}
                value={ipv6}
                onChange={(event) => {
                  setIPv6(event.target.value as IPv6Policy);
                  setSHA256(undefined);
                }}
              >
                <option value="follow">跟随代理</option>
                <option value="direct">直连</option>
                <option value="block">阻断</option>
              </select>
            </Field>
            {(["mixed", "tproxy", "dns"] as const).map((name) => (
              <Field key={name} label={`${name} 监听端口`}>
                <input
                  disabled={runtime.pending || pending || importing}
                  type="number"
                  required
                  min={1}
                  max={65535}
                  value={ports[name]}
                  onChange={(event) => {
                    setPorts((previous) => ({
                      ...previous,
                      [name]: event.target.valueAsNumber,
                    }));
                    setSHA256(undefined);
                  }}
                />
              </Field>
            ))}
          </div>
          <p className="text-muted text-xs">
            故障策略：回退直连。配置保存不会接管 LAN 客户端。
          </p>
          {runtime.error !== undefined && (
            <ErrorState message={errorMessage(runtime.error)} />
          )}
          {sha256 && (
            <p role="status" className="mono wrap">
              配置已校验并保存 · SHA-256 {sha256}
            </p>
          )}
          <div className="form-actions">
            <span className="text-muted text-xs">
              {runtime.status?.artifactAvailable
                ? "运行文件已就绪"
                : "先在运行管理中获取运行文件"}
            </span>
            <Button
              type="submit"
              variant="primary"
              disabled={
                importing ||
                !runtime.enabled ||
                runtime.pending ||
                pending ||
                !valid ||
                !runtime.status?.artifactAvailable
              }
            >
              {pending ? "编译校验中…" : "生成并 Commit 节点配置"}
            </Button>
          </div>
        </div>
      </form>
    </Panel>
  );
}
