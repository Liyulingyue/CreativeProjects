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
  }, []);

  const stopCamera = useCallback(() => {
    setIsCameraReady(false);
  }, []);

  return {
    videoRef,
    isCameraReady,
    cameraError,
    startCamera,
    stopCamera,
  };
}