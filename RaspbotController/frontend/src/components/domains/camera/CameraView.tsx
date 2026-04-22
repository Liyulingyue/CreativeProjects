import { useEffect, useRef, useState } from 'react';
import { cameraApi } from '../../../api';
import './index.css';

export const CameraView = () => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    let aborted = false;
    let pollTimer: ReturnType<typeof setTimeout> | null = null;

    const fetchFrame = async () => {
      if (aborted) return;
      try {
        const resp = await fetch(cameraApi.getSnapshotUrl(80));
        if (!resp.ok || aborted) return;
        const blob = await resp.blob();
        const url = URL.createObjectURL(blob);
        const img = new Image();
        img.onload = () => {
          if (aborted) { URL.revokeObjectURL(url); return; }
          const canvas = canvasRef.current;
          if (!canvas) { URL.revokeObjectURL(url); return; }
          canvas.width = img.width;
          canvas.height = img.height;
          canvas.getContext('2d')!.drawImage(img, 0, 0);
          URL.revokeObjectURL(url);
          setConnected(true);
        };
        img.onerror = () => {
          URL.revokeObjectURL(url);
          setConnected(false);
        };
        img.src = url;
      } catch {
        setConnected(false);
      }
      if (!aborted) {
        pollTimer = setTimeout(fetchFrame, 300);
      }
    };

    fetchFrame();
    return () => {
      aborted = true;
      if (pollTimer) clearTimeout(pollTimer);
    };
  }, []);

  return (
    <div className="camera-container">
      <div className="camera-header">
        <h3>Camera Feed</h3>
        <span className={`status-indicator ${connected ? 'connected' : 'disconnected'}`}>
          {connected ? '● Live' : '○ Connecting...'}
        </span>
      </div>
      <div className="camera-frame">
        <canvas ref={canvasRef} />
      </div>
    </div>
  );
};
