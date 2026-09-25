import { useState, useEffect, useCallback, useRef } from 'react';
import { SETTINGS_KEY, DEFAULT_SETTINGS } from '../utils/constants';

export function useSettings() {
  const [settings, setSettings] = useState(() => {
    try {
      const saved = localStorage.getItem(SETTINGS_KEY);
      return { ...DEFAULT_SETTINGS, ...JSON.parse(saved) };
    } catch {
      return { ...DEFAULT_SETTINGS };
    }
  });

  const update = useCallback((patch) => {
    setSettings((prev) => {
      const next = { ...prev, ...patch };
      localStorage.setItem(SETTINGS_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const saveAll = useCallback((newSettings) => {
    const merged = { ...DEFAULT_SETTINGS, ...newSettings };
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(merged));
    setSettings(merged);
    return merged;
  }, []);

  return { settings, update, saveAll };
}
