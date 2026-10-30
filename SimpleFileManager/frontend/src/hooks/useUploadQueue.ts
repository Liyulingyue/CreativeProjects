import { useCallback, useRef, useState } from 'react';

export type UploadStatus = 'waiting' | 'uploading' | 'done' | 'failed' | 'cancelled';

export interface UploadQueueItem {
  id: string;
  name: string;
  size: number;
  loaded: number;
  status: UploadStatus;
  error?: string;
}

/**
 * The uploads, as a list somebody can watch.
 *
 * Uploading a folder of photographs is dozens of files, and one bar
 * for all of them says nothing about which one is slow or which one
 * failed. Here each file has its own line and its own state, and the
 * whole thing can be stopped -- a NAS that is uploading a hundred
 * gigabytes should be able to be told to stop.
 */
export function useUploadQueue() {
  const [items, setItems] = useState<UploadQueueItem[]>([]);
  const [open, setOpen] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  const cancelledRef = useRef(false);

  const reset = useCallback(() => {
    setItems([]);
    setOpen(false);
    abortRef.current = null;
    cancelledRef.current = false;
  }, []);

  /** The list of files about to be uploaded. */
  const start = useCallback((files: { id: string; name: string; size: number }[]) => {
    cancelledRef.current = false;
    abortRef.current = new AbortController();
    setItems(
      files.map(f => ({
        id: f.id,
        name: f.name,
        size: f.size,
        loaded: 0,
        status: 'waiting' as UploadStatus,
      })),
    );
    setOpen(true);
  }, []);

  const update = useCallback((id: string, patch: Partial<UploadQueueItem>) => {
    setItems(prev => prev.map(it => (it.id === id ? { ...it, ...patch } : it)));
  }, []);

  const setAll = useCallback((status: UploadStatus, error?: string) => {
    setItems(prev => prev.map(it => (
      it.status === 'done' ? it : { ...it, status, error }
    )));
  }, []);

  /** Stop everything: the file in flight and the ones waiting. */
  const cancel = useCallback(() => {
    cancelledRef.current = true;
    abortRef.current?.abort();
    setAll('cancelled');
    abortRef.current = null;
  }, [setAll]);

  const isCancelled = useCallback(() => cancelledRef.current, []);

  const signal = useCallback(() => abortRef.current?.signal, []);

  const totals = items.reduce(
    (acc, it) => ({
      done: acc.done + (it.status === 'done' ? 1 : 0),
      failed: acc.failed + (it.status === 'failed' ? 1 : 0),
      total: acc.total + 1,
      bytes: acc.bytes + it.loaded,
      size: acc.size + it.size,
    }),
    { done: 0, failed: 0, total: 0, bytes: 0, size: 0 },
  );

  return {
    items, open, setOpen, start, update, setAll, cancel, isCancelled, signal,
    reset, totals,
    isUploading: items.some(it => it.status === 'uploading' || it.status === 'waiting'),
  };
}
