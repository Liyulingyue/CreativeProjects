import { useState, useEffect, useRef, useCallback } from "react";
import { updatePresence, getMonitorStatus, startMonitoring, stopMonitoring } from "../api";
import { MonitorSnapshot } from "../types";

interface UseMonitorResult {
  snapshot: MonitorSnapshot | null;
  isMonitoring: boolean;
  toggleMonitoring: () => Promise<void>;
  reportPresence: (present: boolean) => Promise<void>;
}

export function useMonitor(): UseMonitorResult {
  const [snapshot, setSnapshot] = useState<MonitorSnapshot | null>(null);
  const [isMonitoring, setIsMonitoring] = useState(false);
  const isMonitoringRef = useRef(false);
  const initialized = useRef(false);
  const pollTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    getMonitorStatus().then((snap) => {
      setSnapshot(snap);
      if (snap.is_monitoring) {
        setIsMonitoring(true);
        isMonitoringRef.current = true;
      }
    }).catch(() => {});
  }, []);

  useEffect(() => {
    if (isMonitoring) {
      pollTimerRef.current = setInterval(async () => {
        try {
          const snap = await getMonitorStatus();
          setSnapshot(snap);
        } catch {}
      }, 1000);
    } else {
      if (pollTimerRef.current) {
        clearInterval(pollTimerRef.current);
        pollTimerRef.current = null;
      }
    }
    return () => {
      if (pollTimerRef.current) {
        clearInterval(pollTimerRef.current);
        pollTimerRef.current = null;
      }
    };
  }, [isMonitoring]);

  const toggleMonitoring = useCallback(async () => {
    if (isMonitoringRef.current) {
      await stopMonitoring();
      isMonitoringRef.current = false;
      setIsMonitoring(false);
      const snap = await getMonitorStatus();
      setSnapshot(snap);
    } else {
      await startMonitoring();
      isMonitoringRef.current = true;
      setIsMonitoring(true);
      const snap = await getMonitorStatus();
      setSnapshot(snap);
    }
  }, []);

  const reportPresence = useCallback(async (present: boolean) => {
    try {
      const snap = await updatePresence(present);
      setSnapshot(snap);
    } catch {}
  }, []);

  return {
    snapshot,
    isMonitoring,
    toggleMonitoring,
    reportPresence,
  };
}
