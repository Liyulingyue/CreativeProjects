import { useState, useEffect, useRef, useCallback } from 'react';
import TopBar from './components/TopBar';
import CameraStage from './components/CameraStage';
import StatusBar from './components/StatusBar';
import InfoPanel from './components/InfoPanel';
import SettingsDrawer from './components/SettingsDrawer';
import { useCamera } from './hooks/useCamera';
import { useSettings } from './hooks/useSettings';
import { sendFrame } from './utils/api';
import { renderOverlay, clearOverlay } from './utils/overlay';
import { getPath } from './utils/helpers';

export default function App() {
  const { settings, saveAll } = useSettings();
  const { videoRef, ready, error, start, stop, captureFrame } = useCamera();

  const overlayRef = useRef(null);
  const overlayCtxRef = useRef(null);

  const [drawerOpen, setDrawerOpen] = useState(false);
  const [running, setRunning] = useState(false);
  const [status, setStatus] = useState('idle');
  const [statusText, setStatusText] = useState('空闲');
  const [latency, setLatency] = useState(0);
  const [infoContent, setInfoContent] = useState('');
  const [toast, setToast] = useState({ msg: '', type: '', visible: false });

  const runningRef = useRef(false);
  const sendingRef = useRef(false);
  const intervalRef = useRef(null);
  const settingsRef = useRef(settings);

  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  useEffect(() => {
    if (overlayRef.current) {
      overlayCtxRef.current = overlayRef.current.getContext('2d');
    }
  }, []);

  const showToast = useCallback((msg, type = '') => {
    setToast({ msg, type, visible: true });
    setTimeout(() => setToast((t) => ({ ...t, visible: false })), 3000);
  }, []);

  const updateStatus = useCallback((level, text) => {
    setStatus(level);
    setStatusText(text);
  }, []);

  const syncOverlaySize = useCallback(() => {
    const video = videoRef.current;
    const canvas = overlayRef.current;
    if (video && canvas && video.videoWidth > 0) {
      canvas.width = video.videoWidth;
      canvas.height = video.videoHeight;
    }
  }, [videoRef]);

  const handleStartCamera = useCallback(async () => {
    const ok = await start(settingsRef.current.facingMode);
    if (ok) {
      syncOverlaySize();
      updateStatus('ready', '就绪');
      showToast('相机已启动', 'success');
    } else {
      updateStatus('error', '相机错误');
      showToast(`相机启动失败: ${error}`, 'error');
    }
  }, [start, syncOverlaySize, updateStatus, showToast, error]);

  const handleFlip = useCallback(async () => {
    const newFacing = settingsRef.current.facingMode === 'environment' ? 'user' : 'environment';
    saveAll({ ...settingsRef.current, facingMode: newFacing });
    if (ready) {
      const ok = await start(newFacing);
      if (ok) {
        syncOverlaySize();
        showToast('已切换摄像头', 'success');
      }
    }
  }, [ready, start, syncOverlaySize, saveAll, showToast]);

  const renderResult = useCallback((data) => {
    const s = settingsRef.current;

    if (overlayCtxRef.current) {
      renderOverlay(overlayCtxRef.current, data, s.overlayField);
    }

    let content;
    if (s.infoField) {
      const value = getPath(data, s.infoField);
      content = value == null
        ? '(字段路径未匹配，显示完整响应)\n\n' + JSON.stringify(data, null, 2)
        : typeof value === 'string' ? value : JSON.stringify(value, null, 2);
    } else {
      content = JSON.stringify(data, null, 2);
    }
    setInfoContent(content);
  }, []);

  const tick = useCallback(async () => {
    if (sendingRef.current || !videoRef.current?.videoWidth) return;
    sendingRef.current = true;

    try {
      const blob = await captureFrame();
      if (!blob) return;

      const t0 = performance.now();
      const result = await sendFrame(blob, settingsRef.current);
      setLatency(Math.round(performance.now() - t0));

      renderResult(result);
      updateStatus('running', '运行中');
    } catch (err) {
      showToast(err.message, 'error');
      updateStatus('error', err.message);
    } finally {
      sendingRef.current = false;
    }
  }, [captureFrame, renderResult, updateStatus, showToast, videoRef]);

  const startAnalysis = useCallback(() => {
    const s = settingsRef.current;
    if (!s.mockMode && !s.apiUrl) {
      showToast('请先配置 API 地址或开启 Mock 模式', 'error');
      setDrawerOpen(true);
      return;
    }

    runningRef.current = true;
    setRunning(true);
    const interval = 1000 / s.frameRate;
    intervalRef.current = setInterval(tick, interval);
    updateStatus('running', '运行中');
    tick();
  }, [tick, updateStatus, showToast]);

  const stopAnalysis = useCallback(() => {
    runningRef.current = false;
    setRunning(false);
    clearInterval(intervalRef.current);
    intervalRef.current = null;
    setLatency(0);
    updateStatus('ready', '已停止');
  }, [updateStatus]);

  const handleToggle = useCallback(() => {
    if (runningRef.current) stopAnalysis();
    else startAnalysis();
  }, [startAnalysis, stopAnalysis]);

  const handleSave = useCallback((draft) => {
    const oldRate = settingsRef.current.frameRate;
    saveAll(draft);
    setDrawerOpen(false);
    showToast('设置已保存', 'success');

    if (runningRef.current && draft.frameRate !== oldRate) {
      stopAnalysis();
      startAnalysis();
    }
  }, [saveAll, showToast, stopAnalysis, startAnalysis]);

  const handleTest = useCallback(async (draft) => {
    if (!ready) {
      showToast('请先启动相机', 'error');
      return;
    }

    settingsRef.current = draft;
    saveAll(draft);
    showToast('正在测试…');

    try {
      const blob = await captureFrame();
      if (!blob) {
        showToast('截帧失败', 'error');
        return;
      }
      const t0 = performance.now();
      const result = await sendFrame(blob, draft);
      setLatency(Math.round(performance.now() - t0));
      renderResult(result);
      updateStatus('ready', `测试完成 ${Math.round(performance.now() - t0)}ms`);
      showToast(`测试成功 (${Math.round(performance.now() - t0)}ms)`, 'success');
    } catch (err) {
      showToast(err.message, 'error');
      updateStatus('error', err.message);
    }
  }, [ready, captureFrame, saveAll, renderResult, updateStatus, showToast]);

  useEffect(() => {
    const onVisibility = () => {
      if (document.hidden && runningRef.current) {
        stopAnalysis();
        showToast('页面不可见，已暂停');
      }
    };
    document.addEventListener('visibilitychange', onVisibility);
    return () => document.removeEventListener('visibilitychange', onVisibility);
  }, [stopAnalysis, showToast]);

  useEffect(() => {
    return () => stop();
  }, [stop]);

  return (
    <>
      <TopBar onFlip={handleFlip} onSettings={() => setDrawerOpen(true)} />

      <CameraStage
        videoRef={videoRef}
        overlayRef={overlayRef}
        ready={ready}
        error={error}
        onStart={handleStartCamera}
      />

      <StatusBar
        status={status}
        statusText={statusText}
        fps={running ? settings.frameRate : 0}
        latency={latency}
        running={running}
        onToggle={handleToggle}
      />

      <InfoPanel content={infoContent} />

      <SettingsDrawer
        open={drawerOpen}
        settings={settings}
        onSave={handleSave}
        onClose={() => setDrawerOpen(false)}
        onTest={handleTest}
      />

      <div className={`toast ${toast.type} ${toast.visible ? '' : 'hidden'}`}>
        {toast.msg}
      </div>
    </>
  );
}
