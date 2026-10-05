import { useEffect, useRef, useState, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import {
  base64ToBytes,
  bytesToBase64,
  closeTerminalBeacon,
  terminalApi,
  textToBase64,
  type TerminalOpenResponse,
} from "../lib/terminal-api";
import { runRequest, ApiError, errorMessage } from "../lib/api";
import { Badge, Button, ErrorState } from "../components/ui/primitives";
import { RefreshCw, SquareTerminal, XCircle } from "lucide-react";

export function TerminalPanel() {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const sessionRef = useRef<TerminalOpenResponse | null>(null);
  const offsetRef = useRef(0);
  const activeRef = useRef(true);

  const [connected, setConnected] = useState(false);
  const [session, setSession] = useState<TerminalOpenResponse | null>(null);
  const [error, setError] = useState<unknown>();
  const [exitCode, setExitCode] = useState<number | null>(null);

  const sendInput = useCallback(async (data: string) => {
    const current = sessionRef.current;
    if (!current || !data) return;
    try {
      const base64 = textToBase64(data);
      await runRequest(terminalApi.input(current.id, base64));
    } catch (cause) {
      if (activeRef.current) {
        setError(cause);
      }
    }
  }, []);

  const sendBinary = useCallback(async (data: string) => {
    const current = sessionRef.current;
    if (!current || !data) return;
    try {
      const bytes = new Uint8Array(data.length);
      for (let i = 0; i < data.length; i++) {
        bytes[i] = data.charCodeAt(i) & 0xff;
      }
      const base64 = bytesToBase64(bytes);
      await runRequest(terminalApi.input(current.id, base64));
    } catch (cause) {
      if (activeRef.current) {
        setError(cause);
      }
    }
  }, []);

  const closeSession = useCallback(async () => {
    const current = sessionRef.current;
    if (!current) return;
    try {
      await runRequest(terminalApi.close(current.id));
    } catch {
      closeTerminalBeacon(current.id);
    } finally {
      sessionRef.current = null;
      setSession(null);
      setConnected(false);
    }
  }, []);

  const startSession = useCallback(async () => {
    setError(undefined);
    setExitCode(null);
    offsetRef.current = 0;

    // Clean up previous active session before opening new one to prevent 409 busy
    if (sessionRef.current) {
      await closeSession();
    }

    const term = terminalRef.current;
    const fitAddon = fitAddonRef.current;
    if (!term || !fitAddon) return;

    try {
      fitAddon.fit();
      const openResult = await runRequest(
        terminalApi.open({
          cols: term.cols || 80,
          rows: term.rows || 24,
        }),
      );

      if (!activeRef.current) {
        closeTerminalBeacon(openResult.id);
        return;
      }

      sessionRef.current = openResult;
      setSession(openResult);
      setConnected(true);
      term.focus();

      // Serial poll loop: active 100ms, idle 250ms, no parallel requests
      let currentOffset = openResult.offset || 0;
      let idle = true;

      const poll = async () => {
        if (!activeRef.current || sessionRef.current?.id !== openResult.id) {
          return;
        }

        try {
          const res = await runRequest(
            terminalApi.output(openResult.id, currentOffset),
          );

          if (!activeRef.current || sessionRef.current?.id !== openResult.id) {
            return;
          }

          if (res.data) {
            const bytes = base64ToBytes(res.data);
            term.write(bytes);
            idle = false;
          } else {
            idle = true;
          }

          currentOffset = res.nextOffset;
          offsetRef.current = currentOffset;

          if (res.state === "exited") {
            setConnected(false);
            setExitCode(res.exitCode);
            term.write(`\r\n\x1b[33m[终端会话已结束 (代码: ${res.exitCode ?? 0})]\x1b[0m\r\n`);
            sessionRef.current = null;
            return;
          }

          const delay = idle ? 250 : 100;
          setTimeout(poll, delay);
        } catch (cause) {
          if (!activeRef.current || sessionRef.current?.id !== openResult.id) {
            return;
          }

          // If transient network drop, wait and retry
          if (cause instanceof ApiError && cause.code === "network_error") {
            setTimeout(poll, 1000);
            return;
          }

          setError(cause);
          setConnected(false);
        }
      };

      setTimeout(poll, 100);
    } catch (cause) {
      setError(cause);
      setConnected(false);
    }
  }, [closeSession]);

  useEffect(() => {
    activeRef.current = true;

    // Terminal instance
    const term = new Terminal({
      cursorBlink: true,
      cursorStyle: "block",
      fontSize: 13,
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
      theme: {
        background: "#0d1117",
        foreground: "#c9d1d9",
        cursor: "#58a6ff",
        selectionBackground: "rgba(56, 139, 253, 0.4)",
        black: "#484f58",
        red: "#ff7b72",
        green: "#3fb950",
        yellow: "#d29922",
        blue: "#58a6ff",
        magenta: "#bc8cff",
        cyan: "#39c5cf",
        white: "#b1bac4",
        brightBlack: "#6e7681",
        brightRed: "#ffa198",
        brightGreen: "#56d364",
        brightYellow: "#e3b341",
        brightBlue: "#79c0ff",
        brightMagenta: "#d2a8ff",
        brightCyan: "#56d4dd",
        brightWhite: "#f0f6fc",
      },
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);

    terminalRef.current = term;
    fitAddonRef.current = fitAddon;

    if (containerRef.current) {
      term.open(containerRef.current);
      fitAddon.fit();
    }

    // Input handlers
    term.onData((data) => {
      void sendInput(data);
    });

    term.onBinary((data) => {
      void sendBinary(data);
    });

    // ResizeObserver
    let resizeTimer: ReturnType<typeof setTimeout> | undefined;
    const observer = new ResizeObserver(() => {
      if (resizeTimer) clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => {
        if (!containerRef.current || !activeRef.current) return;
        fitAddon.fit();
        const current = sessionRef.current;
        if (current && term.rows > 0 && term.cols > 0) {
          void runRequest(
            terminalApi.resize(current.id, term.rows, term.cols),
          ).catch(() => {});
        }
      }, 100);
    });

    if (containerRef.current) {
      observer.observe(containerRef.current);
    }

    // Start initial session
    void startSession();

    // Cleanup on unmount
    return () => {
      activeRef.current = false;
      observer.disconnect();
      if (resizeTimer) clearTimeout(resizeTimer);

      const current = sessionRef.current;
      if (current) {
        closeTerminalBeacon(current.id);
        sessionRef.current = null;
      }

      term.dispose();
      terminalRef.current = null;
      fitAddonRef.current = null;
    };
  }, [startSession, sendInput, sendBinary]);

  return (
    <div className="page-stack">
      <div className="flex items-center justify-between gap-2 p-2 border-b border-border">
        <div className="flex items-center gap-2">
          <SquareTerminal size={18} className="text-primary" />
          <span className="text-sm font-semibold">终端</span>
          <Badge tone={connected ? "success" : exitCode !== null ? "neutral" : "warning"}>
            {connected ? "已连接" : exitCode !== null ? `已退出 (${exitCode})` : "未连接"}
          </Badge>
          {session && (
            <span className="text-xs font-mono text-muted">
              {session.cols}x{session.rows} · 闲置超时 {session.idleTimeoutSeconds}s
            </span>
          )}
        </div>

        <div className="flex items-center gap-2">
          <Button
            type="button"
            size="small"
            variant="ghost"
            onClick={() => void sendInput("\x03")}
            disabled={!connected}
            title="发送 Ctrl+C 中断信号"
          >
            Ctrl+C
          </Button>

          <Button
            type="button"
            size="small"
            variant="ghost"
            onClick={() => terminalRef.current?.clear()}
          >
            清屏
          </Button>

          {!connected ? (
            <Button
              type="button"
              size="small"
              variant="primary"
              onClick={() => void startSession()}
            >
              <RefreshCw size={13} className="mr-1 inline" />
              重新连接
            </Button>
          ) : (
            <Button
              type="button"
              size="small"
              variant="ghost"
              onClick={() => void closeSession()}
            >
              <XCircle size={13} className="mr-1 inline text-danger" />
              关闭会话
            </Button>
          )}
        </div>
      </div>

      {error !== undefined && (
        <ErrorState
          message={errorMessage(error)}
          onRetry={() => void startSession()}
        />
      )}

      <div
        className="w-full rounded-lg overflow-hidden border border-border bg-[#0d1117] p-2"
        style={{ minHeight: "28rem", height: "36rem" }}
      >
        <div ref={containerRef} className="w-full h-full" />
      </div>
    </div>
  );
}
