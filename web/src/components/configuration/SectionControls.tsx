import { useId, useMemo, useState, type FormEvent } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { ChevronDown, Plus, Trash2, X } from "lucide-react";
import { Badge, Button } from "../ui/primitives";
import type { ConfigurationModule } from "./contracts";
import {
  addNativeSection,
  nativeSectionLabel,
  nativeSections,
  newNativeSectionName,
  removeNativeSection,
  type NativeSection,
} from "./native-document";
import {
  sectionTemplates,
  templateErrors,
  type SectionTemplate,
  type SectionTemplateField,
} from "./section-templates";

type Documents = Partial<Record<ConfigurationModule, string>>;
type Reference = NonNullable<SectionTemplateField["reference"]>;
interface ReferenceChoice {
  value: string;
  label: string;
}
type ReferenceChoices = Record<Reference, ReferenceChoice[]>;

export interface SectionControlsProps {
  module: ConfigurationModule;
  content: string;
  section?: NativeSection;
  documents: Documents;
  disabled?: boolean;
  onChange: (content: string, selectedName?: string) => void;
}

function referenceChoices(
  module: ConfigurationModule,
  content: string,
  documents: Documents,
): ReferenceChoices {
  // The active local buffer wins over a saved document, including an empty one.
  const document = (target: ConfigurationModule) =>
    target === module ? content : (documents[target] ?? "");
  const named = (section: NativeSection) => !section.name.startsWith("匿名 ");
  const choices = (
    sections: NativeSection[],
    value: (section: NativeSection) => string,
  ) => {
    const seen = new Set<string>();
    return sections.flatMap((section) => {
      const name = value(section);
      if (!name || seen.has(name)) return [];
      seen.add(name);
      return [{ value: name, label: nativeSectionLabel(section) }];
    });
  };
  return {
    interface: choices(
      nativeSections(document("network")).filter(
        (section) => section.type === "interface" && named(section),
      ),
      (section) => section.name,
    ),
    radio: choices(
      nativeSections(document("wireless")).filter(
        (section) => section.type === "wifi-device" && named(section),
      ),
      (section) => section.name,
    ),
    zone: choices(
      nativeSections(document("firewall")).filter(
        (section) => section.type === "zone",
      ),
      (section) =>
        section.fields.find(
          (field) => field.kind === "option" && field.name === "name",
        )?.value ?? "",
    ),
  };
}

function initialValues(
  template: SectionTemplate,
  references: ReferenceChoices,
) {
  return Object.fromEntries(
    template.fields.map((field) => {
      const value = field.defaultValue ?? "";
      const observed =
        field.widget !== "reference" ||
        (field.reference &&
          references[field.reference].some((item) => item.value === value));
      return [field.name, observed ? value : ""];
    }),
  );
}

/** Unknown directives cannot be moved into a neighboring section on deletion. */
function hasUnknownDirectives(
  content: string,
  section: NativeSection,
): boolean {
  const fragment = content.slice(section.offset, section.end);
  const localSection = nativeSections(fragment)[0];
  if (!localSection) return true;
  return removeNativeSection(fragment, localSection)
    .split(/\r?\n/)
    .some((line) => line.trim() && !line.trimStart().startsWith("#"));
}

function TemplateInput({
  field,
  value,
  id,
  error,
  references,
  disabled,
  onChange,
}: {
  field: SectionTemplateField;
  value: string;
  id: string;
  error?: string;
  references: ReferenceChoices;
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  const props = {
    id,
    name: field.name,
    className: "configuration-section-input",
    value,
    disabled,
    "aria-required": field.required || undefined,
    "aria-invalid": !!error,
    "aria-describedby": `${id}-hint${error ? ` ${id}-error` : ""}`,
    onChange: (
      event: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>,
    ) => onChange(event.target.value),
  };
  if (field.widget === "select")
    return (
      <select {...props}>
        {!field.required || !value ? <option value="">请选择…</option> : null}
        {field.options?.map((option) => (
          <option value={option.value} key={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    );
  const suggestions = field.reference ? references[field.reference] : [];
  return (
    <>
      <input
        {...props}
        type={
          field.widget === "password"
            ? "password"
            : field.widget === "number"
              ? "number"
              : "text"
        }
        placeholder={field.placeholder}
        autoComplete="off"
        spellCheck={false}
        min={field.widget === "number" ? field.min : undefined}
        max={field.widget === "number" ? field.max : undefined}
        step={field.widget === "number" ? 1 : undefined}
        list={field.widget === "reference" ? `${id}-choices` : undefined}
      />
      {field.widget === "reference" && (
        <datalist id={`${id}-choices`}>
          {suggestions.map((choice) => (
            <option
              value={choice.value}
              key={choice.value}
              label={choice.label}
            >
              {choice.label}
            </option>
          ))}
        </datalist>
      )}
    </>
  );
}

function AddSectionForm({
  template,
  content,
  references,
  disabled,
  onChange,
  onClose,
}: {
  template: SectionTemplate;
  content: string;
  references: ReferenceChoices;
  disabled: boolean;
  onChange: SectionControlsProps["onChange"];
  onClose: () => void;
}) {
  const formId = useId();
  const [values, setValues] = useState(() =>
    initialValues(template, references),
  );
  const [customName, setCustomName] = useState<string>();
  const [submitted, setSubmitted] = useState(false);
  const generatedName = newNativeSectionName(content, template.type);
  const name = customName?.trim() || generatedName;
  const errors = submitted ? templateErrors(template, values) : {};
  const nameError = !/^[A-Za-z0-9_-]{1,128}$/.test(name)
    ? "内部标识只能使用字母、数字、下划线或连字符，最多 128 个字符。"
    : nativeSections(content).some((section) => section.name === name)
      ? "内部标识已被使用，请换一个名称。"
      : undefined;
  const hiddenErrors = template.fields.filter(
    (field) => field.hidden && errors[field.name],
  );
  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (disabled) return;
    setSubmitted(true);
    if (Object.keys(templateErrors(template, values)).length || nameError)
      return;
    const fields = template.fields.flatMap((field) => {
      const value = values[field.name] ?? "";
      // Fixed, hidden defaults participate in validation and serialization too.
      return value.trim() ? [{ name: field.name, value }] : [];
    });
    const updated = addNativeSection(content, template.type, name, fields);
    if (updated === content) return;
    onChange(updated, name);
    onClose();
  }
  return (
    <form className="configuration-section-form" noValidate onSubmit={submit}>
      {template.fields
        .filter((field) => !field.hidden)
        .map((field) => {
          const id = `${formId}-${field.name}`;
          return (
            <div className="configuration-section-field" key={field.name}>
              <label htmlFor={id}>{field.label}</label>
              <TemplateInput
                field={field}
                id={id}
                value={values[field.name] ?? ""}
                error={errors[field.name]}
                references={references}
                disabled={disabled}
                onChange={(value) =>
                  setValues((current) => ({ ...current, [field.name]: value }))
                }
              />
              <p className="configuration-section-hint" id={`${id}-hint`}>
                {field.hint}
                {field.widget === "reference" &&
                  " 可选择已有配置，也可输入厂商配置名称；检查更改时会核对引用。"}
              </p>
              {errors[field.name] && (
                <p
                  className="configuration-section-error"
                  id={`${id}-error`}
                  role="alert"
                >
                  {errors[field.name]}
                </p>
              )}
            </div>
          );
        })}
      <details
        className="configuration-section-advanced"
        open={(submitted && !!nameError) || undefined}
      >
        <summary>高级设置</summary>
        <div className="configuration-section-field">
          <label htmlFor={`${formId}-internal-name`}>
            内部配置标识（可选）
          </label>
          <input
            id={`${formId}-internal-name`}
            className="configuration-section-input"
            value={customName ?? generatedName}
            placeholder={generatedName}
            disabled={disabled}
            spellCheck={false}
            autoComplete="off"
            aria-invalid={submitted && !!nameError}
            aria-describedby={`${formId}-internal-name-hint${submitted && nameError ? ` ${formId}-internal-name-error` : ""}`}
            onChange={(event) => setCustomName(event.target.value)}
          />
          <p
            className="configuration-section-hint"
            id={`${formId}-internal-name-hint`}
          >
            已自动生成，通常无需修改。其他配置可用此标识引用；留空会自动生成。
          </p>
          {submitted && nameError && (
            <p
              className="configuration-section-error"
              id={`${formId}-internal-name-error`}
              role="alert"
            >
              {nameError}
            </p>
          )}
        </div>
      </details>
      {hiddenErrors.map((field) => (
        <p
          className="configuration-section-error"
          role="alert"
          key={field.name}
        >
          {errors[field.name]}
        </p>
      ))}
      <footer className="configuration-dialog-footer">
        <Button type="button" onClick={onClose}>
          取消
        </Button>
        <Button type="submit" variant="primary" disabled={disabled}>
          <Plus size={15} />
          添加到待应用更改
        </Button>
      </footer>
    </form>
  );
}

export function SectionControls({
  module,
  content,
  section,
  documents,
  disabled = false,
  onChange,
}: SectionControlsProps) {
  const [template, setTemplate] = useState<SectionTemplate>();
  const [deleting, setDeleting] = useState<{
    content: string;
    section: NativeSection;
  }>();
  const templates = sectionTemplates(module);
  const references = useMemo(
    () => referenceChoices(module, content, documents),
    [module, content, documents],
  );
  const unknownDirectives = !!section && hasUnknownDirectives(content, section);
  const deleteTemplate =
    deleting && templates.find((item) => item.type === deleting.section.type);
  const deleteSourceChanged = !!deleting && deleting.content !== content;
  return (
    <>
      <div className="configuration-section-actions">
        {!!templates.length && (
          <Dropdown.Root>
            <Dropdown.Trigger asChild>
              <Button type="button" size="small" disabled={disabled}>
                <Plus size={14} />
                添加配置
                <ChevronDown size={14} />
              </Button>
            </Dropdown.Trigger>
            <Dropdown.Portal>
              <Dropdown.Content
                className="configuration-section-menu"
                align="start"
                sideOffset={6}
              >
                {templates.map((item) => (
                  <Dropdown.Item
                    key={item.id}
                    disabled={disabled}
                    onSelect={() => setTemplate(item)}
                  >
                    {item.label}
                  </Dropdown.Item>
                ))}
              </Dropdown.Content>
            </Dropdown.Portal>
          </Dropdown.Root>
        )}
        {section && (
          <Button
            type="button"
            size="small"
            variant="ghost"
            disabled={disabled || unknownDirectives}
            aria-describedby={
              unknownDirectives
                ? "configuration-section-delete-blocked"
                : undefined
            }
            onClick={() => setDeleting({ content, section })}
          >
            <Trash2 size={14} />
            删除当前配置
          </Button>
        )}
        {unknownDirectives && (
          <p
            className="configuration-section-hint"
            id="configuration-section-delete-blocked"
          >
            此配置含有无法识别的厂商指令，请在高级原生编辑中检查后删除，以免影响相邻配置。
          </p>
        )}
      </div>
      <Dialog.Root
        open={!!template}
        onOpenChange={(open) => {
          if (!open) setTemplate(undefined);
        }}
      >
        <Dialog.Portal>
          <Dialog.Overlay className="configuration-dialog-overlay" />
          <Dialog.Content className="configuration-section-dialog">
            <div className="configuration-dialog-header">
              <div>
                <Badge tone="primary">本地更改</Badge>
                <Dialog.Title>添加{template?.label}</Dialog.Title>
              </div>
              <Dialog.Close asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  aria-label="关闭添加配置"
                >
                  <X size={16} />
                </Button>
              </Dialog.Close>
            </div>
            <Dialog.Description>
              {template?.hint} 添加后先检查更改，再应用；不会立即影响路由器。
            </Dialog.Description>
            {template && (
              <AddSectionForm
                key={`${module}-${template.id}`}
                template={template}
                content={content}
                references={references}
                disabled={disabled}
                onChange={onChange}
                onClose={() => setTemplate(undefined)}
              />
            )}
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
      <Dialog.Root
        open={!!deleting}
        onOpenChange={(open) => {
          if (!open) setDeleting(undefined);
        }}
      >
        <Dialog.Portal>
          <Dialog.Overlay className="configuration-dialog-overlay" />
          <Dialog.Content className="configuration-section-dialog">
            <div className="configuration-dialog-header">
              <div>
                <Badge tone="warning">待应用更改</Badge>
                <Dialog.Title>删除配置？</Dialog.Title>
              </div>
              <Dialog.Close asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  aria-label="关闭删除配置"
                >
                  <X size={16} />
                </Button>
              </Dialog.Close>
            </div>
            <Dialog.Description>
              仅从本地待应用更改中删除。删除后先检查更改，再应用；不会立即影响路由器。
            </Dialog.Description>
            {deleting && (
              <div className="configuration-section-delete-summary">
                <strong>
                  {deleteTemplate?.label ?? "当前配置"} ·{" "}
                  {nativeSectionLabel(deleting.section)}
                </strong>
                <p>
                  {deleteTemplate?.deleteImpact ??
                    "应用后，此配置及其设置将不再生效，依赖此配置的连接或服务可能受影响。"}
                </p>
              </div>
            )}
            {deleteSourceChanged && (
              <p className="configuration-section-error" role="alert">
                本地配置已改变，请取消后重新选择要删除的配置。
              </p>
            )}
            <footer className="configuration-dialog-footer">
              <Dialog.Close asChild>
                <Button type="button">取消</Button>
              </Dialog.Close>
              <Button
                type="button"
                className="configuration-danger-button"
                disabled={disabled || deleteSourceChanged}
                onClick={() => {
                  if (!deleting || disabled || deleteSourceChanged) return;
                  const updated = removeNativeSection(
                    content,
                    deleting.section,
                  );
                  if (updated !== content) onChange(updated);
                  setDeleting(undefined);
                }}
              >
                <Trash2 size={15} />
                删除并保留为待应用更改
              </Button>
            </footer>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </>
  );
}
