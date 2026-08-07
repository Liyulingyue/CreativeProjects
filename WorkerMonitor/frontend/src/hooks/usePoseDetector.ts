import { useState, useEffect } from "react";
import { initDetector } from "../api";

interface UsePoseDetectorResult {
  isReady: boolean;
  isLoading: boolean;
  error: string | null;
}

export function usePoseDetector(): UsePoseDetectorResult {
  const [isReady, setIsReady] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setIsLoading(true);
    initDetector()
      .then(() => {
        if (!cancelled) {
          setIsReady(true);
          setError(null);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setError(String(e));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setIsLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return { isReady, isLoading, error };
}