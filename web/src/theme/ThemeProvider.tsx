import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react';

export type ThemeMode = 'light' | 'dark' | 'system';
export type ResolvedTheme = 'light' | 'dark';
export const THEME_STORAGE_KEY = 'be6500panel.theme';
interface ThemeContextValue {
  mode: ThemeMode;
  resolvedTheme: ResolvedTheme;
  setMode: (mode: ThemeMode) => void;
}
const ThemeContext = createContext<ThemeContextValue | null>(null);
const isMode = (value: unknown): value is ThemeMode => value === 'light' || value === 'dark' || value === 'system';
const systemTheme = (): ResolvedTheme => typeof window !== 'undefined' && window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';

export function ThemeProvider({ children, defaultMode = 'system' }: { children: ReactNode; defaultMode?: ThemeMode }) {
  const [mode, updateMode] = useState<ThemeMode>(() => {
    try {
      const stored = typeof window !== 'undefined' ? window.localStorage.getItem(THEME_STORAGE_KEY) : null;
      return isMode(stored) ? stored : defaultMode;
    } catch { return defaultMode; }
  });
  const [system, setSystem] = useState<ResolvedTheme>(systemTheme);
  const resolvedTheme = mode === 'system' ? system : mode;
  const setMode = useCallback((next: ThemeMode) => {
    if (!isMode(next)) return;
    updateMode(next);
    try { window.localStorage.setItem(THEME_STORAGE_KEY, next); } catch { /* Preference is optional in private/restricted browsing. */ }
  }, []);

  useEffect(() => {
    const media = window.matchMedia?.('(prefers-color-scheme: dark)');
    if (!media) return;
    const onChange = () => setSystem(media.matches ? 'dark' : 'light');
    onChange();
    media.addEventListener('change', onChange);
    const onStorage = (event: StorageEvent) => {
      if (event.key === THEME_STORAGE_KEY) updateMode(isMode(event.newValue) ? event.newValue : defaultMode);
    };
    window.addEventListener('storage', onStorage);
    return () => {
      media.removeEventListener('change', onChange);
      window.removeEventListener('storage', onStorage);
    };
  }, [defaultMode]);

  useEffect(() => {
    const element = document.documentElement;
    element.dataset.theme = resolvedTheme;
    element.classList.toggle('dark', resolvedTheme === 'dark');
    element.style.colorScheme = resolvedTheme;
  }, [resolvedTheme]);

  return <ThemeContext.Provider value={{ mode, resolvedTheme, setMode }}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const theme = useContext(ThemeContext);
  if (!theme) throw new Error('useTheme must be used inside ThemeProvider');
  return theme;
}
