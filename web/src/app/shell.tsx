import { useEffect, useState, type ReactNode } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import {
  Activity,
  ChevronRight,
  Command,
  FileText,
  LogOut,
  Menu,
  PanelLeftClose,
  Router,
  Search,
  ShieldCheck,
  X,
} from "lucide-react";
import { Badge, Button, cn, ErrorState } from "../components/ui/primitives";
import { groups, moduleById, modules, type PageId } from "../modules/registry";
import { useConsole } from "./console-context";
import { CommandPalette } from "./command-palette";
import { ThemeMenu } from "./theme-menu";
import { errorMessage } from "../lib/api";
import { timestamp } from "../lib/format";

export function Shell({
  page,
  navigate,
  onLogout,
  authRequired,
  children,
}: {
  page: PageId;
  navigate: (id: PageId) => void;
  onLogout: () => void;
  authRequired: boolean;
  children: ReactNode;
}) {
  const [commandOpen, setCommandOpen] = useState(false);
  const [mobileOpen, setMobileOpen] = useState(false);
  const [collapsed, setCollapsed] = useState(false);
  const { health, connection, system, error, refresh } = useConsole();
  const registration = moduleById(page);
  const reducedMotion = useReducedMotion();
  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setCommandOpen((open) => !open);
      }
      if (
        event.key === "/" &&
        !(
          event.target instanceof HTMLInputElement ||
          event.target instanceof HTMLTextAreaElement
        ) &&
        !event.metaKey &&
        !event.ctrlKey
      ) {
        const input = document.querySelector<HTMLInputElement>(
          '.page-content input[aria-label^="筛选"]',
        );
        if (input) {
          event.preventDefault();
          input.focus();
        }
      }
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);
  const select = (id: PageId) => {
    navigate(id);
    setMobileOpen(false);
  };
  const Navigation = () => (
    <>
      <div className="sidebar-brand">
        <span className="brand-icon">
          <Router size={20} />
        </span>
        {!collapsed && (
          <span>
            <strong>be6500panel</strong>
            <small>控制平面</small>
          </span>
        )}
      </div>
      <nav aria-label="模块导航" className="sidebar-nav">
        {groups.map((group) => (
          <div className="nav-group" key={group.id}>
            {!collapsed && (
              <span className="nav-group-title">{group.title}</span>
            )}
            {modules
              .filter((module) => module.group === group.id)
              .map((module) => (
                <a
                  key={module.id}
                  href={`#/${module.id}`}
                  onClick={(event) => {
                    event.preventDefault();
                    select(module.id);
                  }}
                  title={collapsed ? module.title : undefined}
                  aria-current={page === module.id ? "page" : undefined}
                  className={cn("nav-item", page === module.id && "nav-active")}
                >
                  <module.icon size={18} />
                  {!collapsed && (
                    <>
                      <span>{module.shortTitle}</span>
                      {page === module.id && (
                        <span className="nav-active-dot" />
                      )}
                    </>
                  )}
                </a>
              ))}
          </div>
        ))}
      </nav>
      <div className="sidebar-footer">
        <div className="workspace-indicator">
          <span className="workspace-icon">
            <ShieldCheck size={16} />
          </span>
          {!collapsed && (
            <span>
              <strong>本地工作空间</strong>
              <small>{health?.readOnly ? "观察模式" : "控制模式"}</small>
            </span>
          )}
        </div>
        <div className="sidebar-footer-actions">
          <Button
            variant="ghost"
            size="icon"
            aria-label={collapsed ? "展开导航" : "收起导航"}
            onClick={() => setCollapsed(!collapsed)}
          >
            <PanelLeftClose size={16} />
          </Button>
          {!collapsed && <span>v0.1.0</span>}
        </div>
      </div>
    </>
  );
  return (
    <div className={cn("console-layout", collapsed && "sidebar-collapsed")}>
      <a
        className="skip-link"
        href="#main-content"
        onClick={(event) => {
          event.preventDefault();
          document.getElementById("main-content")?.focus();
        }}
      >
        跳至主要内容
      </a>
      <aside className="desktop-sidebar">
        <Navigation />
      </aside>
      <div className="console-main">
        <header className="topbar">
          <div className="topbar-left">
            <Button
              className="mobile-menu"
              variant="ghost"
              size="icon"
              aria-label="打开导航"
              onClick={() => {
                setCollapsed(false);
                setMobileOpen(true);
              }}
            >
              <Menu size={19} />
            </Button>
            <div className="breadcrumbs">
              <span>工作空间</span>
              <ChevronRight size={13} />
              <strong>{registration.shortTitle}</strong>
            </div>
          </div>
          <button
            className="global-search"
            onClick={() => setCommandOpen(true)}
          >
            <Search size={15} />
            <span>搜索模块与操作</span>
            <kbd>
              <Command size={11} /> K
            </kbd>
          </button>
          <div className="topbar-status">
            <Badge tone={health?.mode === "demo" ? "warning" : "neutral"}>
              {health?.mode === "demo"
                ? "Demo"
                : health?.mode === "host"
                  ? "Host"
                  : "连接中"}
            </Badge>
            <Badge tone={health?.readOnly === false ? "primary" : "neutral"}>{health?.readOnly === false ? "Full control" : "观察"}</Badge>
            <span
              className={cn(
                "connection-indicator",
                connection === "live" && "connection-live",
              )}
              title={`事件流：${connection}`}
              aria-label={`事件流：${connection}`}
            />
            <ThemeMenu />
            {authRequired && (
              <Button
                variant="ghost"
                size="icon"
                aria-label="退出登录"
                onClick={onLogout}
              >
                <LogOut size={16} />
              </Button>
            )}
          </div>
        </header>
        <main className="page-content" id="main-content" tabIndex={-1}>
          <div className="page-heading">
            <div>
              <div className="eyebrow">
                {registration.group === "overview"
                  ? "WORKSPACE"
                  : registration.group === "network"
                    ? "NETWORK"
                    : "SERVICES"}{" "}
                / {page.toUpperCase()}
              </div>
              <h1>{registration.title}</h1>
              <p>{registration.description}</p>
            </div>
            <Button
              size="small"
              variant="secondary"
              onClick={() => {
                select("system");
                window.location.hash = "/system?tab=diagnostics";
              }}
              className="diagnostics-shortcut"
            >
              <FileText size={14} />
              诊断
            </Button>
          </div>
          {error !== undefined && (
            <ErrorState message={errorMessage(error)} onRetry={refresh} />
          )}
          <AnimatePresence mode="wait">
            <motion.div
              key={page}
              initial={{
                opacity: reducedMotion ? 1 : 0,
                y: reducedMotion ? 0 : 5,
              }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: reducedMotion ? 1 : 0 }}
              transition={{ duration: reducedMotion ? 0 : 0.14 }}
            >
              {children}
            </motion.div>
          </AnimatePresence>
          <footer className="console-footer">
            <span>
              <Activity size={12} />
              {connection === "live"
                ? "状态流正常"
                : connection === "paused"
                  ? "后台已暂停"
                  : connection === "offline"
                    ? "连接中断 · 自动重连"
                    : "正在连接状态流"}
            </span>
            <span>采样 {timestamp(system?.sampledAt)}</span>
            <span className="footer-name">be6500panel</span>
          </footer>
        </main>
      </div>
      <CommandPalette
        open={commandOpen}
        setOpen={setCommandOpen}
        navigate={select}
      />
      <Dialog.Root open={mobileOpen} onOpenChange={setMobileOpen}>
        <Dialog.Portal>
          <Dialog.Overlay className="dialog-overlay" />
          <Dialog.Content className="mobile-sidebar">
            <Dialog.Title className="sr-only">模块导航</Dialog.Title>
            <Dialog.Description className="sr-only">
              选择控制台模块
            </Dialog.Description>
            <Dialog.Close className="mobile-close" aria-label="关闭导航">
              <X size={18} />
            </Dialog.Close>
            <Navigation />
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </div>
  );
}
