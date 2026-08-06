import { useState, useCallback, useEffect } from "react";
import { detectFrame, initDetector, updateDetection } from "../api";

interface UsePoseDetectorResult {
  isReady: boolean;
  isLoading: boolean;
  error: string | null;
  detect: () => Promise<{ personDetected: boolean; posture: null }>;
}

export function usePoseDetector(): UsePoseDetectorResult {
  const [isReady, setIsReady] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    initDetector()
      .then(() => setIsReady(true))
      .catch((e) => setError(String(e)));
  }, []);

  const detect = useCallback(async (): Promise<{ personDetected: boolean; posture: null }> => {
    setIsLoading(true);
    try {
      const result = await detectFrame();
      await updateDetection(result);
      return {
        personDetected: result.person_detected,
        posture: null,
      };
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      return { personDetected: false, posture: null };
    } finally {
      setIsLoading(false);
    }
  }, []);

  return { isReady, isLoading, error, detect };
}