import { useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { ArrowUp, CornerDownLeft, Search, SunMoon } from "lucide-react";
import { modules, type PageId } from "../modules/registry";
import { useTheme } from "../theme";
import { cn } from "../components/ui/primitives";

export function CommandPalette({
  open,
  setOpen,
  navigate,
}: {
  open: boolean;
  setOpen: (open: boolean) => void;
  navigate: (id: PageId) => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const { setMode } = useTheme();
  const input = useRef<HTMLInputElement>(null);
  const commands = [
    ...modules.map((module) => ({
      id: module.id,
      title: module.command,
      subtitle: module.description,
      keywords: `${module.title} ${module.keywords}`,
      icon: module.icon,
      action: () => navigate(module.id),
    })),
    ...(["light", "dark", "system"] as const).map((mode) => ({
      id: `theme-${mode}`,
      title: `${{ light: "浅色", dark: "深色", system: "跟随系统" }[mode]}主题`,
      subtitle: "外观设置",
      keywords: `theme ${mode} 主题 外观`,
      icon: SunMoon,
      action: () => setMode(mode),
    })),
  ];
  const filtered = commands.filter((command) =>
    `${command.title} ${command.keywords}`
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  useEffect(() => {
    if (open) {
      setQuery("");
      setActive(0);
    }
  }, [open]);
  const choose = (index: number) => {
    const command = filtered[index];
    if (command) {
      command.action();
      setOpen(false);
    }
  };
  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content
          className="command-dialog"
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            input.current?.focus();
          }}
        >
          <Dialog.Title className="sr-only">命令中心</Dialog.Title>
          <Dialog.Description className="sr-only">
            搜索模块或切换主题，使用方向键选择，回车执行。
          </Dialog.Description>
          <div className="command-search">
            <Search size={19} />
            <input
              ref={input}
              role="combobox"
              aria-expanded={open}
              aria-controls="command-list"
              aria-activedescendant={
                filtered[active] ? `command-${filtered[active].id}` : undefined
              }
              placeholder="搜索模块、操作或设置…"
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                setActive(0);
              }}
              onKeyDown={(event) => {
                if (event.key === "ArrowDown") {
                  event.preventDefault();
                  setActive(
                    (index) => (index + 1) % Math.max(filtered.length, 1),
                  );
                }
                if (event.key === "ArrowUp") {
                  event.preventDefault();
                  setActive(
                    (index) =>
                      (index - 1 + Math.max(filtered.length, 1)) %
                      Math.max(filtered.length, 1),
                  );
                }
                if (event.key === "Enter") {
                  event.preventDefault();
                  choose(active);
                }
              }}
            />
            <kbd>ESC</kbd>
          </div>
          <div
            className="command-list"
            id="command-list"
            role="listbox"
            aria-label="可用命令"
          >
            <div className="command-group">快速导航与操作</div>
            {filtered.map((command, index) => (
              <button
                type="button"
                id={`command-${command.id}`}
                role="option"
                aria-selected={index === active}
                className={cn(
                  "command-item",
                  index === active && "command-active",
                )}
                key={command.id}
                onClick={() => choose(index)}
                onMouseMove={() => setActive(index)}
              >
                <command.icon size={17} />
                <span>
                  <strong>{command.title}</strong>
                  <small>{command.subtitle}</small>
                </span>
                {index === active && <CornerDownLeft size={15} />}
              </button>
            ))}
            {!filtered.length && <div className="empty-inline">无匹配命令</div>}
          </div>
          <div className="command-footer">
            <span>
              <ArrowUp size={12} />↑ ↓ 选择
            </span>
            <span>
              <CornerDownLeft size={12} />
              执行
            </span>
            <span>⌘ / Ctrl + K</span>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
