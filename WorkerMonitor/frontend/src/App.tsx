import { useState, useEffect, useRef, useCallback } from "react";
import { createPortal } from "react-dom";
import { listen } from "@tauri-apps/api/event";
import { useCamera } from "./hooks/useCamera";
import { usePoseDetector } from "./hooks/usePoseDetector";
import { useMonitor } from "./hooks/useMonitor";
import { getConfig, saveConfig, dismissBreakAlert } from "./api";
import { enterCompactMode, enterExpandedMode, hideToTray } from "./utils/windowMode";
import { AppConfig, PostureResult } from "./types";
import MonitorView from "./components/MonitorView";
import CompactView from "./components/CompactView";
import Settings from "./components/Settings";
import BreakAlert from "./components/BreakAlert";
import "./App.css";

export default function App() {
  const { videoRef, canvasRef, isCameraReady, cameraError, startCamera, stopCamera } = useCamera();
  const { isReady: isPoseReady, isLoading: isPoseLoading, error: poseError, detect } = usePoseDetector();
  const { snapshot, isMonitoring, toggleMonitoring } = useMonitor();
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [view, setView] = useState<"monitor" | "settings" | "compact">("monitor");
  const [breakAlertInfo, setBreakAlertInfo] = useState<{ workSecs: number } | null>(null);
  const [welcomeBack, setWelcomeBack] = useState<{ breakSecs: number } | null>(null);
  const [posture, setPosture] = useState<PostureResult | null>(null);
  const [personDetected, setPersonDetected] = useState(false);

  const lastTimestampRef = useRef<number>(0);
  const toggleFnRef = useRef(toggleMonitoring);
  toggleFnRef.current = toggleMonitoring;
  const isMonitoringRef = useRef(isMonitoring);
  isMonitoringRef.current = isMonitoring;

  useEffect(() => {
    getConfig().then(setConfig).catch(() => {});
  }, []);

  useEffect(() => {
    if (!snapshot) return;
    const det = snapshot.detection;
    if (det && det.person_detected) {
      setPersonDetected(true);
      setPosture({
        score: det.score,
        headForward: det.head_forward,
        headTilt: det.head_tilt,
        shoulderUneven: det.shoulder_uneven,
        slouching: det.slouching,
        details: {
          headForwardAngle: 0,
          headTiltAngle: 0,
          shoulderDiff: 0,
          slouchAngle: 0,
        },
      });
    } else {
      setPersonDetected(false);
      setPosture(null);
    }
  }, [snapshot]);

  useEffect(() => {
    const unlisten1 = listen<number>("work-threshold-exceeded", (e) => {
      setBreakAlertInfo({ workSecs: e.payload });
    });
    const unlisten2 = listen<number>("break-ended", (e) => {
      setWelcomeBack({ breakSecs: e.payload });
    });
    const unlisten3 = listen<number>("posture-alert", () => {});
    return () => {
      unlisten1.then((f) => f());
      unlisten2.then((f) => f());
      unlisten3.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (breakAlertInfo) {
      const t = setTimeout(() => setBreakAlertInfo(null), 30000);
      return () => clearTimeout(t);
    }
  }, [breakAlertInfo]);

  useEffect(() => {
    if (welcomeBack) {
      const t = setTimeout(() => setWelcomeBack(null), 5000);
      return () => clearTimeout(t);
    }
  }, [welcomeBack]);

  useEffect(() => {
    if (!(isMonitoring && isCameraReady && isPoseReady)) return;

    const doDetection = async () => {
      const video = videoRef.current;
      if (!video) return;
      const now = performance.now();
      if (now === lastTimestampRef.current) return;
      lastTimestampRef.current = now;

      const result = await detect(video, now);
      setPersonDetected(result.personDetected);
    };

    doDetection();
    const interval = (config?.check_interval_seconds ?? 5) * 1000;
    const timer = setInterval(doDetection, interval);
    return () => clearInterval(timer);
  }, [isMonitoring, isCameraReady, isPoseReady, config, detect]);

  const handleToggleMonitoring = useCallback(async () => {
    const current = isMonitoringRef.current;
    if (!current) {
      await startCamera();
      await toggleFnRef.current();
    } else {
      stopCamera();
      await toggleFnRef.current();
      setPersonDetected(false);
      setPosture(null);
    }
  }, [startCamera, stopCamera]);

  const handleSaveConfig = async (newConfig: AppConfig) => {
    await saveConfig(newConfig);
    setConfig(newConfig);
  };

  const handleDismissAlert = async () => {
    setBreakAlertInfo(null);
    await dismissBreakAlert();
  };

  const handleEnterCompact = async () => {
    await enterCompactMode();
    setView("compact");
  };

  const handleEnterExpanded = async () => {
    await enterExpandedMode();
    setView("monitor");
  };

  const handleHideToTray = async () => {
    await hideToTray();
  };

  const status = snapshot?.status ?? "idle";
  const workSecs = snapshot?.work_duration_secs ?? 0;
  const breakSecs = snapshot?.break_duration_secs ?? 0;
  const totalWork = snapshot?.total_work_secs ?? 0;
  const totalBreak = snapshot?.total_break_secs ?? 0;

  const cameraPortal = createPortal(
    <>
      <video ref={videoRef} playsInline muted style={{ position: "fixed", top: 0, left: 0, width: 0, height: 0, pointerEvents: "none", visibility: "hidden" }} />
      <canvas ref={canvasRef} style={{ display: "none" }} />
    </>,
    document.body
  );

  return (
    <>
      {cameraPortal}

      {view === "compact" && (
        <div className="app-compact">
          <CompactView
            status={status}
            isMonitoring={isMonitoring}
            posture={posture}
            personDetected={personDetected}
            workSecs={workSecs}
            breakSecs={breakSecs}
            onExpand={handleEnterExpanded}
            onToggleMonitoring={handleToggleMonitoring}
            onHide={handleHideToTray}
          />
          {welcomeBack && (
            <div className="welcome-back-toast compact-toast">
              <span className="welcome-back-icon">👋</span>
              <div>
                <strong>欢迎回来！</strong>
                <p>休息了 {Math.floor(welcomeBack.breakSecs / 60)}分</p>
              </div>
            </div>
          )}
        </div>
      )}

      {view === "settings" && (
        <div className="app">
          <header className="app-header">
            <div className="app-header-left">
              <span className="app-logo">🖥️</span>
              <span className="app-header-title">WorkerMonitor</span>
            </div>
            <div className="app-header-right">
              <button className="icon-btn" onClick={() => setView("monitor")} title="返回">←</button>
            </div>
          </header>
          <Settings config={config} onSave={handleSaveConfig} onClose={() => setView("monitor")} />
        </div>
      )}

      {view === "monitor" && (
        <div className="app">
          <MonitorView
            videoRef={videoRef}
            canvasRef={canvasRef}
            isCameraReady={isCameraReady}
            cameraError={cameraError}
            status={status}
            isMonitoring={isMonitoring}
            personDetected={personDetected}
            posture={posture}
            workSecs={workSecs}
            breakSecs={breakSecs}
            totalWork={totalWork}
            totalBreak={totalBreak}
            isPoseReady={isPoseReady}
            isPoseLoading={isPoseLoading}
            poseError={poseError}
            onToggleMonitoring={handleToggleMonitoring}
            onCompact={handleEnterCompact}
            onSettings={() => setView("settings")}
          />
        </div>
      )}

      {breakAlertInfo && (
        <BreakAlert workSecs={breakAlertInfo.workSecs} onDismiss={handleDismissAlert} />
      )}

      {welcomeBack && view !== "compact" && (
        <div className="welcome-back-toast">
          <span className="welcome-back-icon">👋</span>
          <div className="welcome-back-text">
            <strong>欢迎回来！</strong>
            <p>休息了 {Math.floor(welcomeBack.breakSecs / 60)}分</p>
          </div>
        </div>
      )}
    </>
  );
}
