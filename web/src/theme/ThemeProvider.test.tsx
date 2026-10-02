import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ThemeProvider, useTheme, THEME_STORAGE_KEY } from './ThemeProvider';

let dark = false;
let listeners: Set<() => void>;
function Probe() {
  const { mode, resolvedTheme, setMode } = useTheme();
  return <><output>{mode}:{resolvedTheme}</output><button onClick={() => setMode('dark')}>dark</button><button onClick={() => setMode('light')}>light</button><button onClick={() => setMode('system')}>system</button></>;
}
beforeEach(() => {
  window.localStorage.clear();
  dark = false;
  listeners = new Set();
  vi.stubGlobal('matchMedia', vi.fn(() => ({
    get matches() { return dark; },
    addEventListener: (_: string, listener: () => void) => listeners.add(listener),
    removeEventListener: (_: string, listener: () => void) => listeners.delete(listener),
  })));
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

describe('ThemeProvider', () => {
  it('restores a stored preference and applies the resolved theme', () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, 'dark');
    render(<ThemeProvider><Probe /></ThemeProvider>);
    expect(screen.getByText('dark:dark')).toBeTruthy();
    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });
  it('persists only the mode and follows system changes only in system mode', () => {
    render(<ThemeProvider><Probe /></ThemeProvider>);
    act(() => { dark = true; listeners.forEach(listener => listener()); });
    expect(screen.getByText('system:dark')).toBeTruthy();
    fireEvent.click(screen.getByText('light'));
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('light');
    act(() => { dark = false; listeners.forEach(listener => listener()); dark = true; listeners.forEach(listener => listener()); });
    expect(screen.getByText('light:light')).toBeTruthy();
    fireEvent.click(screen.getByText('system'));
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('system');
    expect(screen.getByText('system:dark')).toBeTruthy();
  });
  it('handles corrupt storage, cross-tab changes and listener cleanup', () => {
    localStorage.setItem(THEME_STORAGE_KEY, 'corrupt');
    const view = render(<ThemeProvider defaultMode="light"><Probe /></ThemeProvider>);
    expect(screen.getByText('light:light')).toBeTruthy();
    act(() => { window.dispatchEvent(new StorageEvent('storage', { key: THEME_STORAGE_KEY, newValue: 'dark' })); });
    expect(screen.getByText('dark:dark')).toBeTruthy();
    view.unmount();
    expect(listeners.size).toBe(0);
  });
  it('works when preference storage is blocked', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('blocked'); });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('blocked'); });
    render(<ThemeProvider defaultMode="light"><Probe /></ThemeProvider>);
    fireEvent.click(screen.getByText('dark'));
    expect(screen.getByText('dark:dark')).toBeTruthy();
  });
});
