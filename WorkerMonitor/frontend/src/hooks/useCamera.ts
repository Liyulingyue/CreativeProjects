import { useState, useRef, useCallback } from "react";

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
    if (videoRef.current) {
      videoRef.current.src = "http://127.0.0.1:8080/stream";
    }
  }, []);

  const stopCamera = useCallback(() => {
    setIsCameraReady(false);
    if (videoRef.current) {
      videoRef.current.src = "";
    }
  }, []);

  return {
    videoRef,
    isCameraReady,
    cameraError,
    startCamera,
    stopCamera,
  };
}
