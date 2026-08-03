import { useState, useEffect, useRef, useCallback } from "react";
import { postFrame } from "../api";

export interface PoseDetectionResult {
  personDetected: boolean;
  posture: null;
}

interface UsePoseDetectorResult {
  isReady: boolean;
  isLoading: boolean;
  error: string | null;
  detect: (video: HTMLVideoElement, timestamp: number) => Promise<PoseDetectionResult>;
}

export function usePoseDetector(): UsePoseDetectorResult {
  const lastResultRef = useRef<PoseDetectionResult>({
    personDetected: false,
    posture: null,
  });
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [isReady] = useState(true);
  const [isLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const canvas = document.createElement("canvas");
    canvas.width = 192;
    canvas.height = 256;
    canvasRef.current = canvas;
    return () => {
      canvasRef.current = null;
    };
  }, []);

  const detect = useCallback(
    async (video: HTMLVideoElement, _timestamp: number): Promise<PoseDetectionResult> => {
      const canvas = canvasRef.current;
      if (!canvas) {
        return lastResultRef.current;
      }

      try {
        const ctx = canvas.getContext("2d");
        if (!ctx) return lastResultRef.current;

        ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
        const dataUrl = canvas.toDataURL("image/jpeg", 0.8);

        await postFrame(dataUrl);

        return lastResultRef.current;
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
        return lastResultRef.current;
      }
    },
    []
  );

  return { isReady, isLoading, error, detect };
}
