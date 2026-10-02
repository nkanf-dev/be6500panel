import { Cable } from "lucide-react";
import {
  Badge,
  Field,
  Panel,
  PanelHeader,
  Select,
} from "../components/ui/primitives";
import type { FrpcPlanInput } from "../lib/contracts";

export function FrpcConnectionForm({
  input,
  token,
  disabled,
  onChange,
  onTokenChange,
}: {
  input: FrpcPlanInput;
  token: string;
  disabled: boolean;
  onChange: <K extends keyof FrpcPlanInput>(
    key: K,
    value: FrpcPlanInput[K],
  ) => void;
  onTokenChange: (value: string) => void;
}) {
  return (
    <Panel>
      <PanelHeader
        title="服务器连接"
        subtitle="填写自己的 frps 服务器；最终 Commit 执行原生校验，不会自动启动或暴露面板"
        action={<Cable size={16} />}
      />
      <div className="config-form">
        {!input.serverAddress.trim() && (
          <span className="status-text">
            <Badge>待配置</Badge>
            <span className="text-muted text-xs">尚未填写 frps 服务器</span>
          </span>
        )}
        <Field label="服务器地址">
          <input
            placeholder="frps.example.com"
            autoComplete="off"
            required
            disabled={disabled}
            value={input.serverAddress}
            onChange={(event) => onChange("serverAddress", event.target.value)}
          />
        </Field>
        <div className="form-grid">
          <Field label="服务器端口">
            <input
              type="number"
              min={1}
              max={65535}
              required
              disabled={disabled}
              value={Number.isNaN(input.serverPort) ? "" : input.serverPort}
              onChange={(event) =>
                onChange("serverPort", event.target.valueAsNumber)
              }
            />
          </Field>
          <Field label="传输协议">
            <Select
              label="传输协议"
              value={input.transport}
              onValueChange={(value) => {
                if (!disabled)
                  onChange("transport", value as FrpcPlanInput["transport"]);
              }}
              options={[
                { value: "tcp", label: "TCP" },
                { value: "quic", label: "QUIC" },
              ]}
            />
          </Field>
        </div>
        <Field
          label="认证令牌"
          hint="仅填写服务器提供的 token；无令牌则不生成认证配置。Commit 成功后清空输入。"
        >
          <input
            aria-label="认证令牌"
            type="password"
            autoComplete="new-password"
            spellCheck={false}
            disabled={disabled}
            value={token}
            onChange={(event) => onTokenChange(event.target.value)}
          />
        </Field>
        <label className="checkbox-field">
          <input
            type="checkbox"
            disabled={disabled}
            checked={input.tls}
            onChange={(event) => onChange("tls", event.target.checked)}
          />
          <span>
            <strong>启用 TLS</strong>
            <small>加密客户端到服务器的连接</small>
          </span>
        </label>
      </div>
    </Panel>
  );
}
