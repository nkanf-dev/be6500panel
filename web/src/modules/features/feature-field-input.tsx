import { useState, useId, type ChangeEvent } from "react";
import { Eye, EyeOff, Lock } from "lucide-react";
import { Badge, Field } from "../../components/ui/primitives";
import type { FeatureField } from "../../lib/features-api";

function isTruthy(val: unknown): boolean {
  if (
    val === false ||
    val === 0 ||
    val === "0" ||
    val === "false" ||
    val === "off" ||
    val === "disabled" ||
    val === null ||
    val === undefined ||
    val === ""
  ) {
    return false;
  }
  return Boolean(val);
}

interface FeatureFieldInputProps {
  field: FeatureField;
  value: unknown;
  configured?: boolean;
  disabled?: boolean;
  error?: string;
  onChange: (value: unknown) => void;
}

export function FeatureFieldInput({
  field,
  value,
  configured = false,
  disabled = false,
  error,
  onChange,
}: FeatureFieldInputProps) {
  const [showSecret, setShowSecret] = useState(false);
  const inputId = useId();

  switch (field.kind) {
    case "boolean": {
      const checked = isTruthy(value);
      return (
        <label htmlFor={inputId} className="flex items-center gap-2 cursor-pointer py-1">
          <input
            id={inputId}
            aria-label={field.label}
            type="checkbox"
            checked={checked}
            disabled={disabled}
            onChange={(e) => onChange(e.target.checked)}
            className="rounded border-border text-primary focus:ring-primary"
          />
          <span className="font-medium text-sm">{field.label}</span>
          {field.required && <span className="text-danger text-xs">*</span>}
        </label>
      );
    }

    case "select": {
      const stringValue = String(value ?? "");
      const options = field.options ?? [];
      return (
        <Field label={field.label} hint={error}>
          <select
            id={inputId}
            aria-label={field.label}
            className="select-trigger w-full"
            value={stringValue}
            disabled={disabled}
            onChange={(e) => onChange(e.target.value)}
          >
            {!field.required && <option value="">未选择</option>}
            {options.map((opt) => (
              <option key={opt} value={opt}>
                {opt}
              </option>
            ))}
          </select>
        </Field>
      );
    }

    case "integer": {
      const numValue =
        typeof value === "number"
          ? value
          : typeof value === "string" && value.trim() !== "" && !Number.isNaN(Number(value))
            ? Number(value)
            : "";
      return (
        <Field
          label={field.label}
          hint={
            error ??
            (field.min !== undefined || field.max !== undefined
              ? `范围：${field.min ?? "—"} 至 ${field.max ?? "—"}`
              : undefined)
          }
        >
          <input
            id={inputId}
            aria-label={field.label}
            type="number"
            className="input w-full"
            value={numValue}
            min={field.min ?? undefined}
            max={field.max ?? undefined}
            step={1}
            disabled={disabled}
            onChange={(e: ChangeEvent<HTMLInputElement>) => {
              const val = e.target.value === "" ? undefined : Number(e.target.value);
              onChange(val);
            }}
          />
        </Field>
      );
    }

    case "secret": {
      const secretValue = typeof value === "string" ? value : "";
      return (
        <Field
          label={field.label}
          hint={
            error ??
            (configured
              ? "已设置密码；留空表示保留原密码不变"
              : field.required
                ? "必填项"
                : undefined)
          }
        >
          <div className="relative flex items-center">
            <input
              id={inputId}
              aria-label={field.label}
              type={showSecret ? "text" : "password"}
              className="input w-full pr-16"
              value={secretValue}
              placeholder={configured ? "已配置（留空保留）" : "请输入密码或密钥"}
              disabled={disabled}
              onChange={(e) => onChange(e.target.value)}
            />
            <div className="absolute right-2 flex items-center gap-1">
              {configured && !secretValue && (
                <Badge tone="neutral" className="text-xs py-0.5">
                  <Lock size={10} className="mr-0.5 inline" /> 已保留
                </Badge>
              )}
              <button
                type="button"
                className="p-1 text-muted hover:text-foreground"
                onClick={() => setShowSecret(!showSecret)}
                aria-label={showSecret ? "隐藏密码" : "显示密码"}
              >
                {showSecret ? <EyeOff size={14} /> : <Eye size={14} />}
              </button>
            </div>
          </div>
        </Field>
      );
    }

    case "ipv4":
    case "ipv6":
    case "mac": {
      const strVal = typeof value === "string" ? value : "";
      const placeholder =
        field.kind === "ipv4"
          ? "例如 192.168.31.1"
          : field.kind === "ipv6"
            ? "例如 2001:db8::1"
            : "例如 AA:BB:CC:DD:EE:FF";
      return (
        <Field label={field.label} hint={error}>
          <input
            id={inputId}
            aria-label={field.label}
            type="text"
            className="input w-full font-mono text-sm"
            value={strVal}
            placeholder={placeholder}
            disabled={disabled}
            onChange={(e) => {
              let val = e.target.value.trim();
              if (field.kind === "mac") val = val.toUpperCase();
              onChange(val);
            }}
          />
        </Field>
      );
    }

    case "json": {
      const jsonStr =
        typeof value === "string"
          ? value
          : typeof value === "object" && value !== null
            ? JSON.stringify(value, null, 2)
            : "";
      return (
        <Field label={field.label} hint={error ?? "JSON 格式配置数据"}>
          <textarea
            id={inputId}
            aria-label={field.label}
            className="textarea w-full font-mono text-xs"
            rows={4}
            value={jsonStr}
            disabled={disabled}
            onChange={(e) => onChange(e.target.value)}
          />
        </Field>
      );
    }

    case "text":
    default: {
      const textVal = typeof value === "string" ? value : String(value ?? "");
      return (
        <Field label={field.label} hint={error}>
          <input
            id={inputId}
            aria-label={field.label}
            type="text"
            className="input w-full"
            value={textVal}
            disabled={disabled}
            onChange={(e) => onChange(e.target.value)}
          />
        </Field>
      );
    }
  }
}
