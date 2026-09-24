import { useState, useCallback, useRef } from 'react';

export function useCamera() {
  const [stream, setStream] = useState(null);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState(null);
  const videoRef = useRef(null);
  const streamRef = useRef(null);

  const start = useCallback(async (facingMode) => {
    try {
      if (streamRef.current) {
        streamRef.current.getTracks().forEach((t) => t.stop());
      }

      const newStream = await navigator.mediaDevices.getUserMedia({
        video: {
          facingMode,
          width: { ideal: 1280 },
          height: { ideal: 720 },
        },
        audio: false,
      });

      streamRef.current = newStream;
      setStream(newStream);

      if (videoRef.current) {
        videoRef.current.srcObject = newStream;
        await videoRef.current.play();
        await new Promise((resolve) => {
          if (videoRef.current.videoWidth > 0) return resolve();
          videoRef.current.addEventListener('loadedmetadata', resolve, { once: true });
        });
      }

      setReady(true);
      setError(null);
      return true;
    } catch (err) {
      setError(err.message);
      setReady(false);
      return false;
    }
  }, []);

  const stop = useCallback(() => {
    if (streamRef.current) {
      streamRef.current.getTracks().forEach((t) => t.stop());
      streamRef.current = null;
    }
    setStream(null);
    setReady(false);
  }, []);

  const captureFrame = useCallback((quality = 0.8) => {
    const video = videoRef.current;
    if (!video || !video.videoWidth) return null;

    const canvas = document.createElement('canvas');
    canvas.width = video.videoWidth;
    canvas.height = video.videoHeight;
    const ctx = canvas.getContext('2d');
    ctx.drawImage(video, 0, 0);

    return new Promise((resolve) => canvas.toBlob(resolve, 'image/jpeg', quality));
  }, []);

  return { stream, ready, error, videoRef, start, stop, captureFrame };
}
