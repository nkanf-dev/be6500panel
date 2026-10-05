import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { TerminalPanel } from "./terminal";
import * as terminalApiModule from "../lib/terminal-api";
import { Effect } from "effect";

// Hoisted mock classes and methods so Vitest hoisting never hits uninitialized variables
const termMocks = vi.hoisted(() => {
  let binaryHandler: ((data: string) => void) | undefined;
  const write = vi.fn();
  const clear = vi.fn();
  const focus = vi.fn();
  const dispose = vi.fn();
  const loadAddon = vi.fn();
  const open = vi.fn();
  const fit = vi.fn();

  class MockTerminal {
    cols = 80;
    rows = 24;
    loadAddon = loadAddon;
    open = open;
    write = write;
    clear = clear;
    focus = focus;
    dispose = dispose;
    onData = vi.fn(() => ({ dispose: vi.fn() }));
    onBinary = vi.fn((cb: (d: string) => void) => {
      binaryHandler = cb;
      return { dispose: vi.fn() };
    });
  }

  class MockFitAddon {
    fit = fit;
  }

  return {
    MockTerminal,
    MockFitAddon,
    write,
    clear,
    focus,
    dispose,
    loadAddon,
    open,
    fit,
    getBinaryHandler: () => binaryHandler,
    reset: () => {
      binaryHandler = undefined;
      write.mockReset();
      clear.mockReset();
      focus.mockReset();
      dispose.mockReset();
      loadAddon.mockReset();
      open.mockReset();
      fit.mockReset();
    },
  };
});

vi.mock("@xterm/xterm", () => ({
  Terminal: termMocks.MockTerminal,
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: termMocks.MockFitAddon,
}));

// Shared setup already installs a stable ResizeObserver class.
beforeEach(() => {
  termMocks.reset();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("TerminalPanel", () => {
  const openFixture: terminalApiModule.TerminalOpenResponse = {
    id: "term-session-1",
    rows: 24,
    cols: 80,
    term: "xterm-256color",
    offset: 0,
    idleTimeoutSeconds: 300,
  };

  it("opens terminal session on mount and renders connected badge", async () => {
    vi.spyOn(terminalApiModule.terminalApi, "open").mockReturnValue(
      Effect.succeed(openFixture),
    );
    vi.spyOn(terminalApiModule.terminalApi, "output").mockReturnValue(
      Effect.succeed({
        id: "term-session-1",
        data: "",
        offset: 0,
        nextOffset: 0,
        truncated: false,
        state: "open",
        exitCode: null,
      }),
    );

    render(<TerminalPanel />);

    await waitFor(() => {
      expect(screen.getByText("已连接")).toBeInTheDocument();
      expect(screen.getByText(/80x24/)).toBeInTheDocument();
    });
  });

  it("sends base64 input and Ctrl+C raw byte", async () => {
    vi.spyOn(terminalApiModule.terminalApi, "open").mockReturnValue(
      Effect.succeed(openFixture),
    );
    vi.spyOn(terminalApiModule.terminalApi, "output").mockReturnValue(
      Effect.succeed({
        id: "term-session-1",
        data: "",
        offset: 0,
        nextOffset: 0,
        truncated: false,
        state: "open",
        exitCode: null,
      }),
    );
    const inputSpy = vi.spyOn(terminalApiModule.terminalApi, "input").mockReturnValue(
      Effect.succeed({ accepted: 1 }),
    );

    render(<TerminalPanel />);

    await waitFor(() => {
      expect(screen.getByText("已连接")).toBeInTheDocument();
    });

    // Click Ctrl+C button
    const ctrlCBtn = screen.getByRole("button", { name: "Ctrl+C" });
    fireEvent.click(ctrlCBtn);

    await waitFor(() => {
      expect(inputSpy).toHaveBeenCalledWith("term-session-1", expect.any(String));
    });
  });

  it("handles exited state, cleans up session, allows reopening, and preserves binary input bytes", async () => {
    let openCount = 0;
    const openSpy = vi.spyOn(terminalApiModule.terminalApi, "open").mockImplementation(() => {
      openCount++;
      return Effect.succeed({
        ...openFixture,
        id: `term-session-${openCount}`,
      });
    });

    let outputCalls = 0;
    vi.spyOn(terminalApiModule.terminalApi, "output").mockImplementation(() => {
      outputCalls++;
      // Call 1 & 2: keep session open while we send binary input
      if (outputCalls < 3) {
        return Effect.succeed({
          id: "term-session-1",
          data: "",
          offset: 0,
          nextOffset: 0,
          truncated: false,
          state: "open",
          exitCode: null,
        });
      }
      // Call 3+: emit exited state
      return Effect.succeed({
        id: "term-session-1",
        data: btoa("process exited\n"),
        offset: 0,
        nextOffset: 15,
        truncated: false,
        state: "exited",
        exitCode: 0,
      });
    });

    const closeSpy = vi.spyOn(terminalApiModule.terminalApi, "close").mockReturnValue(
      Effect.succeed({ state: "closed" }),
    );

    const inputSpy = vi.spyOn(terminalApiModule.terminalApi, "input").mockReturnValue(
      Effect.succeed({ accepted: 4 }),
    );

    render(<TerminalPanel />);

    // 1. Wait for session to connect
    await waitFor(() => {
      expect(screen.getByText("已连接")).toBeInTheDocument();
    });

    // 2. While session is open and active, send binary input bytes [0xff, 0xfe, 0x00, 0x7f]
    // Base64 encoding of [0xff, 0xfe, 0x00, 0x7f] is '//4Afw=='
    const binaryHandler = termMocks.getBinaryHandler();
    if (binaryHandler) {
      binaryHandler(String.fromCharCode(0xff, 0xfe, 0x00, 0x7f));
      await waitFor(() => {
        expect(inputSpy).toHaveBeenCalledWith("term-session-1", "//4Afw==");
      });
    }

    // 3. Wait for session to transition to exited state
    await waitFor(() => {
      expect(termMocks.write).toHaveBeenCalled();
      expect(screen.getByText(/已退出 \(0\)/)).toBeInTheDocument();
    });

    // 4. Click 重新连接 to verify clean reopening without busy state
    const reconnectBtn = screen.getByRole("button", { name: "重新连接" });
    fireEvent.click(reconnectBtn);

    await waitFor(() => {
      expect(openSpy).toHaveBeenCalledTimes(2);
    });

    // 5. Click 关闭会话 to verify close API is called with new session
    const closeBtn = screen.getByRole("button", { name: "关闭会话" });
    fireEvent.click(closeBtn);

    await waitFor(() => {
      expect(closeSpy).toHaveBeenCalledWith("term-session-2");
    });
  });
});
