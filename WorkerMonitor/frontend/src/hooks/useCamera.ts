import { useState, useRef, useCallback, useEffect } from "react";
import { getFrameBase64 } from "../api";

interface UseCameraResult {
  videoRef: React.RefObject<HTMLImageElement | null>;
  isCameraReady: boolean;
  cameraError: string | null;
  startCamera: () => Promise<void>;
  stopCamera: () => void;
}

export function useCamera(): UseCameraResult {
  const videoRef = useRef<HTMLImageElement | null>(null);
  const [isCameraReady, setIsCameraReady] = useState(false);
  const [cameraError, setCameraError] = useState<string | null>(null);

  const startCamera = useCallback(async () => {
    setCameraError(null);
    setIsCameraReady(true);
  }, []);

  const stopCamera = useCallback(() => {
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
        if (!cancelled && videoRef.current) {
          videoRef.current.src = `data:image/jpeg;base64,${frame}`;
          setCameraError((prev) => (prev ? null : prev));
        }
      } catch (err) {
        if (!cancelled) {
          const msg = err instanceof Error ? err.message : String(err);
          if (!msg.includes("no frame available")) {
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