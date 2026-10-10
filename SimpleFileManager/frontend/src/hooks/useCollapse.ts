import { useCallback, useEffect, useState } from 'react';

/**
 * Sidebar collapse state persisted in localStorage, so the layout a user set
 * up once survives a reload. localStorage throws in private mode / blocked
 * cookies, hence the try/catch — a lost preference is not worth breaking on.
 */
export function useCollapse(key: string, defaultCollapsed = false) {
  const [collapsed, setCollapsed] = useState<boolean>(() => {
    try {
      const raw = localStorage.getItem(key);
      return raw === null ? defaultCollapsed : raw === '1';
    } catch {
      return defaultCollapsed;
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem(key, collapsed ? '1' : '0');
    } catch {
      // ignore
    }
  }, [key, collapsed]);

  const toggle = useCallback(() => setCollapsed(v => !v), []);

  return { collapsed, setCollapsed, toggle };
}