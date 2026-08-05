import { useState, useCallback } from "react";
import { postFrame } from "../api";

export interface PoseDetectionResult {
  personDetected: boolean;
  posture: null;
}

interface UsePoseDetectorResult {
  isReady: boolean;
  isLoading: boolean;
  error: string | null;
  detect: () => Promise<PoseDetectionResult>;
}

export function usePoseDetector(): UsePoseDetectorResult {
  const [isReady] = useState(true);
  const [isLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const detect = useCallback(async (): Promise<PoseDetectionResult> => {
    try {
      const result = await postFrame("");
      return {
        personDetected: result.person_detected ?? false,
        posture: null,
      };
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      return { personDetected: false, posture: null };
    }
  }, []);

  return { isReady, isLoading, error, detect };
}
