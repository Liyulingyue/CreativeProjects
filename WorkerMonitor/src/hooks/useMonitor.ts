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
  const initialized = useRef(false);

  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    getMonitorStatus().then(setSnapshot).catch(() => {});
  }, []);

  const toggleMonitoring = useCallback(async () => {
    if (isMonitoring) {
      await stopMonitoring();
      setIsMonitoring(false);
      const snap = await getMonitorStatus();
      setSnapshot(snap);
    } else {
      await startMonitoring();
      setIsMonitoring(true);
      const snap = await getMonitorStatus();
      setSnapshot(snap);
    }
  }, [isMonitoring]);

  const reportPresence = useCallback(async (present: boolean) => {
    try {
      const snap = await updatePresence(present);
      setSnapshot(snap);
      setIsMonitoring(snap.is_monitoring);
    } catch {
      // ignore
    }
  }, []);

  return {
    snapshot,
    isMonitoring,
    toggleMonitoring,
    reportPresence,
  };
}
