import { useState, useRef, useCallback, useEffect } from "react";
import { getFrameBase64 } from "../api";

const MAX_TRANSIENT_FAILURES = 6;

interface UseCameraResult {
  videoRef: React.RefObject<HTMLImageElement | null>;
  isCameraReady: boolean;
  cameraError: string | null;
  startCamera: () => Promise<void>;
  stopCamera: () => void;
}

export function useCamera(): UseCameraResult {
  const videoRef = useRef<HTMLImageElement | null>(null);
  const failureCountRef = useRef(0);
  const [isCameraReady, setIsCameraReady] = useState(false);
  const [cameraError, setCameraError] = useState<string | null>(null);

  const startCamera = useCallback(async () => {
    failureCountRef.current = 0;
    setCameraError(null);
    setIsCameraReady(true);
  }, []);

  const stopCamera = useCallback(() => {
    failureCountRef.current = 0;
    setIsCameraReady(false);
  }, []);

  useEffect(() => {
    if (!isCameraReady) {
      if (videoRef.current) {
        videoRef.current.removeAttribute("src");
      }
      return;
    }

    let cancelled = false;
    let inFlight = false;

    const pullFrame = async () => {
      if (cancelled || inFlight) {
        return;
      }

      inFlight = true;
      try {
        const frame = await getFrameBase64();
        failureCountRef.current = 0;
        if (!cancelled && videoRef.current) {
          videoRef.current.src = `data:image/jpeg;base64,${frame}`;
          setCameraError((prev) => (prev ? null : prev));
        }
      } catch (err) {
        if (!cancelled) {
          const msg = err instanceof Error ? err.message : String(err);
          const isTransient = msg.includes("no frame available") || msg.includes("IPC timeout");
          if (isTransient) {
            failureCountRef.current += 1;
          }

          if (!isTransient || failureCountRef.current >= MAX_TRANSIENT_FAILURES) {
            setCameraError(msg);
          }
        }
      } finally {
        inFlight = false;
      }
    };

    pullFrame();
    const timer = setInterval(pullFrame, 120);

    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [isCameraReady]);

  return {
    videoRef,
    isCameraReady,
    cameraError,
    startCamera,
    stopCamera,
  };
}