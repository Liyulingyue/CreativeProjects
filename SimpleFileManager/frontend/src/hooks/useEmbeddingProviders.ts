import { useCallback, useEffect, useState } from 'react';
import type { EmbeddingProvider, EmbeddingProviderCreate, EmbeddingProviderUpdate } from '../types';
import {
  createEmbeddingProvider, deleteEmbeddingProvider,
  listEmbeddingProviders, updateEmbeddingProvider,
} from '../api';

/**
 * Owns the embedding provider list state. Same shape as
 * useProviders — the LLM card and the embedding card now share
 * the same CRUD vocabulary, so the same hook pattern applies.
 *
 * Resolution rules on the server: the row with is_index=1 is
 * what the background indexer uses; the row with is_search=1 is
 * what the runtime search/chat path uses (or the index row when
 * search is unset). The hook just exposes the list — callers
 * that need "the index row" or "the search row" call the
 * helpers below.
 */
export function useEmbeddingProviders() {
  const [providers, setProviders] = useState<EmbeddingProvider[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    const list = await listEmbeddingProviders();
    setProviders(list);
    return list;
  }, []);

  useEffect(() => {
    (async () => {
      try {
        await refresh();
      } catch (e) {
        console.error('Failed to load embedding providers', e);
      } finally {
        setIsLoading(false);
      }
    })();
  }, [refresh]);

  const add = useCallback(async (input: EmbeddingProviderCreate) => {
    const created = await createEmbeddingProvider(input);
    await refresh();
    return created;
  }, [refresh]);

  const update = useCallback(async (id: string, patch: EmbeddingProviderUpdate) => {
    const updated = await updateEmbeddingProvider(id, patch);
    await refresh();
    return updated;
  }, [refresh]);

  const remove = useCallback(async (id: string) => {
    await deleteEmbeddingProvider(id);
    await refresh();
  }, [refresh]);

  const indexProvider = providers.find(p => p.is_index) ?? null;
  const searchProvider = providers.find(p => p.is_search) ?? null;

  return { providers, indexProvider, searchProvider, isLoading, refresh, add, update, remove };
}
