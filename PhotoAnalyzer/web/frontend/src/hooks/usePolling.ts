import { useState, useEffect, useCallback } from "react";

export function usePolling<T>(
  fetcher: () => Promise<T>,
  options: {
    interval?: number;
    active?: boolean;
  } = {}
) {
  const { interval = 2000, active = true } = options;
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetch = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const result = await fetcher();
      setData(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  }, [fetcher]);

  useEffect(() => {
    if (!active) return;
    fetch();
    const timer = setInterval(fetch, interval);
    return () => clearInterval(timer);
  }, [fetch, interval, active]);

  return { data, loading, error, refetch: fetch };
}
