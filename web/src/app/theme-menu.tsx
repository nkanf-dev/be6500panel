import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { Check, Monitor, Moon, Sun } from "lucide-react";
import { useTheme } from "../theme";
import { Button } from "../components/ui/primitives";
export function ThemeMenu() {
  const { mode, resolvedTheme, setMode } = useTheme();
  return (
    <Dropdown.Root>
      <Dropdown.Trigger asChild>
        <Button variant="ghost" size="icon" aria-label="切换主题">
          {resolvedTheme === "dark" ? <Moon size={17} /> : <Sun size={17} />}
        </Button>
      </Dropdown.Trigger>
      <Dropdown.Portal>
        <Dropdown.Content
          className="dropdown-content"
          align="end"
          sideOffset={8}
        >
          <Dropdown.Label className="dropdown-label">外观</Dropdown.Label>
          {(
            [
              { value: "light", label: "浅色", icon: Sun },
              { value: "dark", label: "深色", icon: Moon },
              { value: "system", label: "跟随系统", icon: Monitor },
            ] as const
          ).map((option) => (
            <Dropdown.Item
              className="dropdown-item"
              key={option.value}
              onSelect={() => setMode(option.value)}
            >
              <option.icon size={15} />
              <span>{option.label}</span>
              {mode === option.value && <Check size={14} />}
            </Dropdown.Item>
          ))}
        </Dropdown.Content>
      </Dropdown.Portal>
    </Dropdown.Root>
  );
}
