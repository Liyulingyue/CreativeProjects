import { useState, useEffect, useRef, useCallback } from "react";
import { FilesetResolver, PoseLandmarker } from "@mediapipe/tasks-vision";
import { PostureResult, analyzePosture } from "../utils/postureAnalysis";

export interface PoseDetectionResult {
  personDetected: boolean;
  posture: PostureResult | null;
}

interface UsePoseDetectorResult {
  isReady: boolean;
  isLoading: boolean;
  error: string | null;
  detect: (video: HTMLVideoElement, timestamp: number) => PoseDetectionResult;
}

const MODEL_URL =
  "https://storage.googleapis.com/mediapipe-models/pose_landmarker/pose_landmarker_full/float16/latest/pose_landmarker_full.task";

const WASM_URL = "https://cdn.jsdelivr.net/npm/@mediapipe/tasks-vision@latest/wasm";

export function usePoseDetector(): UsePoseDetectorResult {
  const landmarkerRef = useRef<PoseLandmarker | null>(null);
  const [isReady, setIsReady] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lastResultRef = useRef<PoseDetectionResult>({
    personDetected: false,
    posture: null,
  });

  useEffect(() => {
    let cancelled = false;

    async function load() {
      setIsLoading(true);
      setError(null);
      try {
        const vision = await FilesetResolver.forVisionTasks(WASM_URL);
        if (cancelled) return;

        const landmarker = await PoseLandmarker.createFromOptions(vision, {
          baseOptions: {
            modelAssetPath: MODEL_URL,
            delegate: "GPU",
          },
          runningMode: "VIDEO",
          numPoses: 1,
          minPoseDetectionConfidence: 0.5,
          minPosePresenceConfidence: 0.5,
          minTrackingConfidence: 0.5,
        });
        if (cancelled) {
          landmarker.close();
          return;
        }

        landmarkerRef.current = landmarker;
        setIsReady(true);
        setIsLoading(false);
      } catch (err) {
        if (cancelled) return;
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
        setIsLoading(false);
      }
    }

    load();

    return () => {
      cancelled = true;
      if (landmarkerRef.current) {
        landmarkerRef.current.close();
        landmarkerRef.current = null;
      }
    };
  }, []);

  const detect = useCallback(
    (video: HTMLVideoElement, timestamp: number): PoseDetectionResult => {
      if (!landmarkerRef.current || !isReady) {
        return lastResultRef.current;
      }

      try {
        const results = landmarkerRef.current.detectForVideo(video, timestamp);
        const personDetected = !!(results.landmarks && results.landmarks.length > 0);

        let posture: PostureResult | null = null;
        if (personDetected && results.landmarks) {
          const lm = results.landmarks[0];
          posture = analyzePosture(lm as Array<{ x: number; y: number; z: number; visibility?: number }>);
        }

        const result: PoseDetectionResult = { personDetected, posture };
        lastResultRef.current = result;
        return result;
      } catch {
        return lastResultRef.current;
      }
    },
    [isReady]
  );

  return { isReady, isLoading, error, detect };
}
