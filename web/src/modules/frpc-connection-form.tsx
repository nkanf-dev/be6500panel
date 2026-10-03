import { Cable } from "lucide-react";
import {
  Badge,
  Field,
  Panel,
  PanelHeader,
  Select,
} from "../components/ui/primitives";
import type { FrpcTokenIntent } from "./frpc-document";
import type { FrpcPlanInput } from "../lib/contracts";

export function FrpcConnectionForm({
  input,
  token,
  disabled,
  hasToken,
  onChange,
  onTokenChange,
}: {
  input: FrpcPlanInput;
  token: FrpcTokenIntent;
  hasToken: boolean;
  disabled: boolean;
  onChange: <K extends keyof FrpcPlanInput>(
    key: K,
    value: FrpcPlanInput[K],
  ) => void;
  onTokenChange: (value: FrpcTokenIntent) => void;
}) {
  return (
    <Panel>
      <PanelHeader
        title="服务器连接"
        subtitle="填写自己的 frps 服务器；保存时执行原生校验，不会自动启动或暴露面板"
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
        <fieldset className="config-form" disabled={disabled}>
          <legend>认证密钥</legend>
          <div role="radiogroup" aria-label="密钥操作">
            {[
              { mode: "preserve", label: "保留已保存密钥" },
              { mode: "replace", label: "替换密钥" },
              { mode: "clear", label: "清除密钥" },
            ].map(({ mode, label }) => (
              <label key={mode} className="checkbox-field">
                <input
                  type="radio"
                  name="frpc-token-intent"
                  value={mode}
                  checked={token.mode === mode}
                  onChange={() =>
                    onTokenChange({
                      mode: mode as FrpcTokenIntent["mode"],
                      value: "",
                    })
                  }
                />
                <span>{label}</span>
              </label>
            ))}
          </div>
          <Field
            label="认证令牌"
            hint={
              token.mode === "clear"
                ? "保存后清除已保存密钥；未修改的认证扩展参数仍会保留。"
                : "留空默认保留已保存密钥；选择替换并填写新密钥，或明确选择清除。"
            }
          >
            <input
              aria-label="认证令牌"
              type="password"
              autoComplete="new-password"
              spellCheck={false}
              disabled={disabled || token.mode === "clear"}
              placeholder={
                hasToken ? "已配置密钥（留空保持不变）" : "未配置密钥（可留空）"
              }
              value={token.value}
              onChange={(event) =>
                onTokenChange({
                  mode: event.target.value ? "replace" : "preserve",
                  value: event.target.value,
                })
              }
            />
          </Field>
        </fieldset>
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
