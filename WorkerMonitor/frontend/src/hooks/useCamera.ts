import { useState, useRef, useCallback, useEffect } from "react";
import { getFrameBase64 } from "../api";
import { CAMERA_STREAM_URL } from "../utils/cameraStream";

const MAX_TRANSIENT_FAILURES = 6;
export type CameraTransportMode = "idle" | "stream" | "fallback-polling";

interface UseCameraResult {
  videoRef: (node: HTMLImageElement | null) => void;
  isCameraReady: boolean;
  cameraError: string | null;
  cameraTransportMode: CameraTransportMode;
  startCamera: () => Promise<void>;
  stopCamera: () => void;
}

export function useCamera(): UseCameraResult {
  const videoNodeRef = useRef<HTMLImageElement | null>(null);
  const [videoBindVersion, setVideoBindVersion] = useState(0);
  const failureCountRef = useRef(0);
  const [isCameraReady, setIsCameraReady] = useState(false);
  const [cameraError, setCameraError] = useState<string | null>(null);
  const [cameraTransportMode, setCameraTransportMode] = useState<CameraTransportMode>("idle");
  const transportModeRef = useRef<CameraTransportMode>("idle");

  const setTransportMode = useCallback((mode: CameraTransportMode) => {
    if (transportModeRef.current === mode) return;
    transportModeRef.current = mode;
    setCameraTransportMode(mode);
    console.info(`[camera] transport mode => ${mode}`);
  }, []);

  const videoRef = useCallback((node: HTMLImageElement | null) => {
    videoNodeRef.current = node;
    setVideoBindVersion(v => v + 1);
  }, []);

  const startCamera = useCallback(async () => {
    failureCountRef.current = 0;
    setCameraError(null);
    setIsCameraReady(true);
    setTransportMode("idle");
  }, [setTransportMode]);

  const stopCamera = useCallback(() => {
    failureCountRef.current = 0;
    setIsCameraReady(false);
    setTransportMode("idle");
  }, [setTransportMode]);

  useEffect(() => {
    if (!isCameraReady) {
      if (videoNodeRef.current) {
        videoNodeRef.current.removeAttribute("src");
      }
      setTransportMode("idle");
      return;
    }

    const img = videoNodeRef.current;
    if (!img) {
      return;
    }

    let cancelled = false;
    let fallbackStarted = false;
    let inFlight = false;
    let fallbackTimer: ReturnType<typeof setInterval> | null = null;
    let fallbackArmTimer: ReturnType<typeof setTimeout> | null = null;

    const startPollingFallback = () => {
      if (cancelled || fallbackStarted) return;
      fallbackStarted = true;
      setTransportMode("fallback-polling");

      const pullFrame = async () => {
        if (cancelled || inFlight) return;
        inFlight = true;
        try {
          const frame = await getFrameBase64();
          failureCountRef.current = 0;
          if (!cancelled && img) {
            img.src = `data:image/jpeg;base64,${frame}`;
            setCameraError(null);
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
      fallbackTimer = setInterval(pullFrame, 33);
    };

    const handleLoad = () => {
      failureCountRef.current = 0;
      setCameraError(null);
      if (!fallbackStarted) {
        setTransportMode("stream");
      }
      if (fallbackArmTimer) {
        clearTimeout(fallbackArmTimer);
        fallbackArmTimer = null;
      }
    };

    const handleError = () => {
      failureCountRef.current += 1;
      startPollingFallback();
    };

    img.addEventListener("load", handleLoad);
    img.addEventListener("error", handleError);
    img.src = `${CAMERA_STREAM_URL}?t=${Date.now()}`;

    // Some WebView environments block local HTTP stream fetches from custom scheme.
    // Only fall back when the stream still has no decoded frame after a grace period.
    fallbackArmTimer = setTimeout(() => {
      const noDecodedFrameYet = !img.naturalWidth || !img.naturalHeight;
      if (noDecodedFrameYet) {
        startPollingFallback();
      }
    }, 5000);

    return () => {
      cancelled = true;
      img.removeEventListener("load", handleLoad);
      img.removeEventListener("error", handleError);
      if (fallbackArmTimer) {
        clearTimeout(fallbackArmTimer);
      }
      if (fallbackTimer) {
        clearInterval(fallbackTimer);
      }
    };
  }, [isCameraReady, setTransportMode, videoBindVersion]);

  return {
    videoRef,
    isCameraReady,
    cameraError,
    cameraTransportMode,
    startCamera,
    stopCamera,
  };
}