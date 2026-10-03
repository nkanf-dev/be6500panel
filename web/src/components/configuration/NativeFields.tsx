import { useId, useMemo, useState } from "react";
import { Info, Plus, Trash2 } from "lucide-react";
import { Button } from "../ui/primitives";
import type { ConfigurationModule } from "./contracts";
import { firmwareContext } from "./field-help";
import type { FieldHelp } from "./field-help/types";
import {
  fieldSchema,
  sectionFields,
  sectionSchema,
  type FieldSchema,
} from "./field-schema";
import {
  addNativeField,
  nativeSectionLabel,
  editNativeField,
  removeNativeField,
  type NativeField,
  type NativeSection,
} from "./native-document";

const trueValues = ["1", "true", "yes", "on"];
const falseValues = ["0", "false", "no", "off"];
function booleanValue(current: string, checked: boolean) {
  const index = Math.max(
    0,
    trueValues.indexOf(current.toLowerCase()),
    falseValues.indexOf(current.toLowerCase()),
  );
  const value = (checked ? trueValues : falseValues)[index];
  return current === current.toUpperCase() ? value.toUpperCase() : value;
}

function FieldInput({
  schema,
  value,
  label,
  id,
  hintId,
  onChange,
}: {
  schema: FieldSchema;
  value: string;
  label: string;
  id: string;
  hintId: string;
  onChange: (value: string) => void;
}) {
  const props = { id, "aria-label": label, "aria-describedby": hintId };
  if (
    schema.widget === "boolean" &&
    [...trueValues, ...falseValues].includes(value.toLowerCase())
  )
    return (
      <label className="configuration-checkbox">
        <input
          {...props}
          type="checkbox"
          checked={trueValues.includes(value.toLowerCase())}
          onChange={(event) =>
            onChange(booleanValue(value, event.target.checked))
          }
        />
        <span>{trueValues.includes(value.toLowerCase()) ? "是" : "否"}</span>
      </label>
    );
  if (schema.widget === "select") {
    const options = schema.options ?? [];
    return (
      <select
        {...props}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        {!options.some((option) => option.value === value) && (
          <option value={value}>{value || "（空值）"} · 当前值</option>
        )}
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    );
  }
  if (schema.widget === "text" && /[\r\n]/.test(value))
    return (
      <textarea
        {...props}
        value={value}
        rows={3}
        spellCheck={false}
        onChange={(event) => onChange(event.target.value)}
      />
    );
  // Vendor variants may use ranges, units, or "auto" in normally numeric fields.
  // Keep them editable without allowing an HTML number input to erase the value.
  const numeric =
    schema.widget === "number" &&
    (value === "" || /^-?\d+(?:\.\d+)?$/.test(value));
  return (
    <input
      {...props}
      type={
        schema.widget === "password" ? "password" : numeric ? "number" : "text"
      }
      value={value}
      min={numeric ? schema.min : undefined}
      max={numeric ? schema.max : undefined}
      step={numeric ? (schema.step ?? 1) : undefined}
      placeholder={schema.placeholder}
      spellCheck={false}
      autoComplete="off"
      onChange={(event) => onChange(event.target.value)}
    />
  );
}

const helpFlagLabels = {
  generated: "固件可能重建此值；仍可编辑",
  "hardware-dependent": "取决于硬件 / 驱动",
  credential: "凭据；说明不展示保存的值",
  legacy: "旧版字段",
  "version-dependent": "需核对版本与消费者",
} as const;

function FieldHelpDetails({ help, label }: { help: FieldHelp; label: string }) {
  return (
    <details className="configuration-field-help">
      <summary>
        <Info size={13} aria-hidden="true" />
        {label}使用说明
      </summary>
      <div className="configuration-field-help-body">
        <p>{help.description}</p>
        <dl>
          {help.defaultValue !== undefined && (
            <>
              <dt>源码回退值</dt>
              <dd>{help.defaultValue}（条件见说明；不是当前值）</dd>
            </>
          )}
          {help.unit && (
            <>
              <dt>单位</dt>
              <dd>{help.unit}</dd>
            </>
          )}
          {help.range && (
            <>
              <dt>格式 / 范围</dt>
              <dd>{help.range}</dd>
            </>
          )}
          {help.dependencies?.length ? (
            <>
              <dt>相关条件</dt>
              <dd>
                <ul>
                  {help.dependencies.map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
              </dd>
            </>
          ) : null}
          <dt>应用后的影响</dt>
          <dd>{help.impact}</dd>
          {help.flags?.length ? (
            <>
              <dt>字段性质</dt>
              <dd>
                {help.flags.map((flag) => helpFlagLabels[flag]).join("；")}
              </dd>
            </>
          ) : null}
        </dl>
        {help.discovery && (
          <p className="configuration-field-help-discovery">
            静态追踪结果：{help.discovery}
          </p>
        )}
        <details className="configuration-field-evidence">
          <summary>固件来源与版本</summary>
          <p>{firmwareContext}</p>
          <ul>
            {help.evidence.map((item, index) => (
              <li key={`${item.source}-${item.line}-${index}`}>
                <code>
                  {item.source}:{item.line}
                  {item.endLine ? `–${item.endLine}` : ""}
                </code>
                <span>{item.fact}</span>
                <small>{item.firmware}</small>
                {item.artifact && (
                  <small>
                    提取自 <code>{item.artifact.source}</code> · SHA-256{" "}
                    {item.artifact.sha256}
                  </small>
                )}
              </li>
            ))}
          </ul>
        </details>
      </div>
    </details>
  );
}

interface FieldGroup {
  key: string;
  name: string;
  kind: NativeField["kind"];
  fields: NativeField[];
}
export function NativeFields({
  module,
  section,
  content,
  onChange,
}: {
  module: ConfigurationModule;
  section: NativeSection;
  content: string;
  onChange: (content: string) => void;
}) {
  const formId = useId();
  const [emptyLists, setEmptyLists] = useState<string[]>([]);
  const [newName, setNewName] = useState("");
  const [newKind, setNewKind] = useState<NativeField["kind"]>("option");
  const [custom, setCustom] = useState(false);
  const groups = useMemo(() => {
    const result: FieldGroup[] = [];
    const lists = new Map<string, FieldGroup>();
    section.fields.forEach((field) => {
      const existing =
        field.kind === "list" ? lists.get(field.name) : undefined;
      if (existing) existing.fields.push(field);
      else {
        const group = {
          key: `${field.kind}-${field.name}-${result.length}`,
          name: field.name,
          kind: field.kind,
          fields: [field],
        };
        result.push(group);
        if (field.kind === "list") lists.set(field.name, group);
      }
    });
    emptyLists.forEach((name) => {
      if (!lists.has(name))
        result.push({ key: `empty-${name}`, name, kind: "list", fields: [] });
    });
    return result;
  }, [section.fields, emptyLists]);
  const available = sectionFields(module, section.type).filter(
    (name) => !section.fields.some((field) => field.name === name),
  );
  const sectionInfo = sectionSchema(module, section.type);
  const validName =
    /^[A-Za-z0-9_]+$/.test(newName) &&
    !section.fields.some((field) => field.name === newName);
  return (
    <div
      className="configuration-field-form"
      aria-label={`${module} ${section.name} 字段编辑`}
    >
      <header className="configuration-field-heading">
        <h3>
          {sectionInfo.label} · {nativeSectionLabel(section)}
        </h3>
        <p>{sectionInfo.hint} 编辑只保留在本地；检查通过后可应用更改。</p>
      </header>
      {groups.map((group, groupIndex) => {
        const schema = fieldSchema(module, section.type, group.name);
        const id = `${formId}-${groupIndex}`;
        const label = `${schema.label} (${group.name})`;
        return (
          <div className="configuration-field" key={group.key}>
            <div className="configuration-field-label">
              {group.kind === "option" ? (
                <label htmlFor={id}>{schema.label}</label>
              ) : (
                <strong>{schema.label}</strong>
              )}
              <code>
                {group.name} · {group.kind}
              </code>
              <p id={`${id}-hint`}>{schema.hint}</p>
              {schema.help && (
                <FieldHelpDetails help={schema.help} label={schema.label} />
              )}
            </div>
            <div className="configuration-field-controls">
              {group.fields.map((field, index) => (
                <div className="configuration-field-item" key={index}>
                  <FieldInput
                    schema={schema}
                    value={field.value}
                    id={group.kind === "list" ? `${id}-${index}` : id}
                    hintId={`${id}-hint`}
                    label={
                      group.kind === "list" ? `${label} ${index + 1}` : label
                    }
                    onChange={(value) =>
                      onChange(editNativeField(content, field, value))
                    }
                  />
                  {group.kind === "list" && (
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      aria-label={`移除 ${group.name} ${index + 1}`}
                      onClick={() => {
                        setEmptyLists((names) =>
                          names.includes(group.name)
                            ? names
                            : [...names, group.name],
                        );
                        onChange(removeNativeField(content, field));
                      }}
                    >
                      <Trash2 size={14} />
                    </Button>
                  )}
                </div>
              ))}
              {group.kind === "list" && (
                <Button
                  type="button"
                  size="small"
                  variant="ghost"
                  aria-label={`添加 ${group.name} 值`}
                  onClick={() =>
                    onChange(
                      addNativeField(content, section, group.name, "list"),
                    )
                  }
                >
                  <Plus size={14} />
                  添加一项
                </Button>
              )}
            </div>
          </div>
        );
      })}
      {!groups.length && (
        <p className="configuration-form-note">
          此章节暂无字段。可在下方添加设置，无需编辑配置文件。
        </p>
      )}
      <div className="configuration-add-field">
        <label htmlFor={`${formId}-add-name`}>添加设置</label>
        {custom ? (
          <input
            id={`${formId}-add-name`}
            aria-label="新字段名称"
            value={newName}
            placeholder="字段名称，如 vendor_option"
            onChange={(event) => setNewName(event.target.value)}
          />
        ) : (
          <select
            id={`${formId}-add-name`}
            aria-label="新字段名称"
            value={newName}
            onChange={(event) => {
              if (event.target.value === "__custom") {
                setCustom(true);
                setNewName("");
              } else setNewName(event.target.value);
            }}
          >
            <option value="">选择设置…</option>
            {available.map((name) => (
              <option key={name} value={name}>
                {fieldSchema(module, section.type, name).label} ({name})
              </option>
            ))}
            <option value="__custom">自定义 / 厂商字段…</option>
          </select>
        )}
        <select
          aria-label="新字段类型"
          value={newKind}
          onChange={(event) =>
            setNewKind(event.target.value as NativeField["kind"])
          }
        >
          <option value="option">单值设置</option>
          <option value="list">多值列表</option>
        </select>
        <Button
          type="button"
          size="small"
          disabled={!validName}
          onClick={() => {
            const schema = fieldSchema(module, section.type, newName);
            onChange(
              addNativeField(
                content,
                section,
                newName,
                newKind,
                schema.widget === "boolean" ? "0" : "",
              ),
            );
            if (newKind === "list")
              setEmptyLists((names) => [...names, newName]);
            setNewName("");
            setCustom(false);
          }}
        >
          <Plus size={14} />
          添加字段
        </Button>
        <p>
          保留未知厂商设置。自定义名称仅用字母、数字或下划线；新值由“检查更改”校验。
        </p>
      </div>
    </div>
  );
}
