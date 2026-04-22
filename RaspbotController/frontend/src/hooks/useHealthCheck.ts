import { useState, useEffect, useRef } from 'react';
import { healthApi } from '../api';
import type { HealthStatus } from '../api';

export const useHealthCheck = () => {
  const [health, setHealth] = useState<HealthStatus | null>(null);
  const isFirstRun = useRef(true);

  useEffect(() => {
    if (isFirstRun.current) {
      isFirstRun.current = false;
      healthApi.get().then(setHealth).catch(() => setHealth(null));
    }

    const interval = setInterval(async () => {
      try {
        const data = await healthApi.get();
        setHealth(data);
      } catch {
        setHealth(null);
      }
    }, 5000);

    return () => clearInterval(interval);
  }, []);

  return health;
};
