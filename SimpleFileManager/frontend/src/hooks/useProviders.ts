import { useCallback, useEffect, useState } from 'react';
import type { LLMProvider, LLMProviderCreate, LLMProviderUpdate } from '../types';
import { createProvider, deleteProvider, listProviders, setMainProvider, updateProvider } from '../api';

/**
 * Owns the provider list state. Fetches on mount and exposes CRUD
 * helpers that refresh after each successful mutation so multiple
 * components (SettingsPage, SimpleChat) stay in sync without
 * prop-drilling.
 *
 * isLoading gates the first paint so the chat dropdown can render
 * "加载中…" instead of flashing "无 main provider" during the
 * initial request.
 */
export function useProviders() {
  const [providers, setProviders] = useState<LLMProvider[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    const list = await listProviders();
    setProviders(list);
    return list;
  }, []);

  useEffect(() => {
    (async () => {
      try {
        await refresh();
      } catch (e) {
        // Surfaced to the consumer; the chat page will show an error
        // banner and the settings page will block on retry.
        console.error('Failed to load providers', e);
      } finally {
        setIsLoading(false);
      }
    })();
  }, [refresh]);

  const add = useCallback(async (input: LLMProviderCreate) => {
    const created = await createProvider(input);
    await refresh();
    return created;
  }, [refresh]);

  const update = useCallback(async (id: string, patch: LLMProviderUpdate) => {
    const updated = await updateProvider(id, patch);
    await refresh();
    return updated;
  }, [refresh]);

  const remove = useCallback(async (id: string) => {
    await deleteProvider(id);
    await refresh();
  }, [refresh]);

  const setMain = useCallback(async (id: string) => {
    const updated = await setMainProvider(id);
    await refresh();
    return updated;
  }, [refresh]);

  const main = providers.find(p => p.is_main) ?? null;
  const byId = (id: string | null | undefined): LLMProvider | null =>
    id ? providers.find(p => p.id === id) ?? null : null;

  return { providers, main, isLoading, refresh, add, update, remove, setMain, byId };
}
