import { useState, useEffect, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { useCamera } from "./hooks/useCamera";
import { usePoseDetector } from "./hooks/usePoseDetector";
import { useMonitor } from "./hooks/useMonitor";
import { getConfig, saveConfig, dismissBreakAlert, reportPosture } from "./api";
import { enterCompactMode, enterExpandedMode, hideToTray } from "./utils/windowMode";
import { AppConfig, PostureResult } from "./types";
import MonitorView from "./components/MonitorView";
import CompactView from "./components/CompactView";
import Settings from "./components/Settings";
import BreakAlert from "./components/BreakAlert";
import "./App.css";

function formatDuration(secs: number): string {
  const total = Math.floor(secs);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

export default function App() {
  const { videoRef, canvasRef, isCameraReady, cameraError, startCamera, stopCamera } = useCamera();
  const { isReady: isPoseReady, isLoading: isPoseLoading, error: poseError, detect } = usePoseDetector();
  const { snapshot, isMonitoring, toggleMonitoring, reportPresence } = useMonitor();
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [compact, setCompact] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [breakAlertInfo, setBreakAlertInfo] = useState<{ workSecs: number } | null>(null);
  const [welcomeBack, setWelcomeBack] = useState<{ breakSecs: number } | null>(null);
  const [posture, setPosture] = useState<PostureResult | null>(null);
  const [personDetected, setPersonDetected] = useState(false);

  const checkTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const tickTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const lastTimestampRef = useRef<number>(0);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    getConfig().then(setConfig).catch(() => {});
  }, []);

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

  const runDetection = useCallback(() => {
    const video = videoRef.current;
    if (!video || !isCameraReady || !isMonitoring || !isPoseReady) return;

    const now = performance.now();
    if (now === lastTimestampRef.current) return;
    lastTimestampRef.current = now;

    const result = detect(video, now);
    setPersonDetected(result.personDetected);
    setPosture(result.posture);

    reportPresence(result.personDetected);

    if (result.personDetected && result.posture) {
      reportPosture(
        result.posture.score,
        result.posture.headForward,
        result.posture.headTilt,
        result.posture.shoulderUneven,
        result.posture.slouching
      ).catch(() => {});
    }
  }, [videoRef, isCameraReady, isMonitoring, isPoseReady, detect, reportPresence]);

  useEffect(() => {
    if (isMonitoring && isCameraReady && isPoseReady) {
      const interval = (config?.check_interval_seconds ?? 5) * 1000;
      runDetection();
      checkTimerRef.current = setInterval(runDetection, interval);
      return () => {
        if (checkTimerRef.current) clearInterval(checkTimerRef.current);
      };
    } else {
      if (checkTimerRef.current) clearInterval(checkTimerRef.current);
    }
  }, [isMonitoring, isCameraReady, isPoseReady, config, runDetection]);

  useEffect(() => {
    if (isMonitoring) {
      tickTimerRef.current = setInterval(() => setTick((t) => t + 1), 1000);
      return () => {
        if (tickTimerRef.current) clearInterval(tickTimerRef.current);
      };
    } else {
      if (tickTimerRef.current) clearInterval(tickTimerRef.current);
    }
  }, [isMonitoring]);

  const handleToggleMonitoring = async () => {
    if (!isMonitoring) {
      await startCamera();
      await toggleMonitoring();
    } else {
      stopCamera();
      await toggleMonitoring();
      setPersonDetected(false);
      setPosture(null);
    }
  };

  const handleSaveConfig = async (newConfig: AppConfig) => {
    await saveConfig(newConfig);
    setConfig(newConfig);
  };

  const handleDismissAlert = async () => {
    setBreakAlertInfo(null);
    await dismissBreakAlert();
  };

  const handleEnterCompact = async () => {
    setShowSettings(false);
    await enterCompactMode();
    setCompact(true);
  };

  const handleEnterExpanded = async () => {
    await enterExpandedMode();
    setCompact(false);
  };

  const handleHideToTray = async () => {
    await hideToTray();
  };

  void tick;

  const status = snapshot?.status ?? "idle";
  const workSecs = snapshot?.work_duration_secs ?? 0;
  const breakSecs = snapshot?.break_duration_secs ?? 0;
  const totalWork = snapshot?.total_work_secs ?? 0;
  const totalBreak = snapshot?.total_break_secs ?? 0;

  if (compact) {
    return (
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
            <div className="welcome-back-content">
              <span className="welcome-back-icon">👋</span>
              <div>
                <strong>欢迎回来！</strong>
                <p>休息了 {formatDuration(welcomeBack.breakSecs)}</p>
              </div>
            </div>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>WorkerMonitor</h1>
        <div className="header-actions">
          <button className="header-btn" onClick={handleEnterCompact} title="紧凑模式">
            ◻
          </button>
          <button className="settings-btn" onClick={() => setShowSettings(!showSettings)}>
            ⚙
          </button>
        </div>
      </header>

      {showSettings ? (
        <Settings config={config} onSave={handleSaveConfig} onClose={() => setShowSettings(false)} />
      ) : (
        <>
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
          />

          {breakAlertInfo && (
            <BreakAlert workSecs={breakAlertInfo.workSecs} onDismiss={handleDismissAlert} />
          )}

          {welcomeBack && (
            <div className="welcome-back-toast">
              <div className="welcome-back-content">
                <span className="welcome-back-icon">👋</span>
                <div>
                  <strong>欢迎回来！</strong>
                  <p>你休息了 {formatDuration(welcomeBack.breakSecs)}</p>
                </div>
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}
