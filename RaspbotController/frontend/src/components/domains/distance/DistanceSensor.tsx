import { useState, useCallback, useEffect, useRef } from 'react';
import { carApi } from '../../../api';
import './index.css';

export const DistanceSensor = () => {
  const [distance, setDistance] = useState<number | null>(null);
  const [status, setStatus] = useState<'idle' | 'measuring' | 'error'>('idle');
  const [autoMode, setAutoMode] = useState(false);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const measure = useCallback(async () => {
    setStatus('measuring');
    try {
      const result = await carApi.getDistance();
      if (result.status === 'ok') {
        setDistance(result.distance);
        setStatus('idle');
      } else {
        setDistance(null);
        setStatus('error');
      }
    } catch {
      setDistance(null);
      setStatus('error');
    }
  }, []);

  const startAutoMeasure = useCallback(() => {
    if (intervalRef.current) return;
    setAutoMode(true);
    measure();
    intervalRef.current = setInterval(measure, 500);
  }, [measure]);

  const stopAutoMeasure = useCallback(() => {
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }
    setAutoMode(false);
  }, []);

  useEffect(() => {
    return () => {
      stopAutoMeasure();
      setAutoMode(false);
    };
  }, [stopAutoMeasure]);

  const getDistanceColor = (d: number | null) => {
    if (d === null || d < 0) return 'danger';
    if (d < 20) return 'danger';
    if (d < 50) return 'warning';
    return 'safe';
  };

  const getBarWidth = (d: number | null) => {
    if (d === null || d < 0) return 0;
    return Math.min(100, (d / 150) * 100);
  };

  return (
    <div className="distance-container">
      <div className="distance-header">
        <h3>Ultrasonic</h3>
        <div className="distance-actions">
          <button className="measure-btn" onClick={measure}>
            MEASURE
          </button>
          <button
            className={`auto-btn ${autoMode ? 'active' : ''}`}
            onClick={() => autoMode ? stopAutoMeasure() : startAutoMeasure()}
          >
            {autoMode ? 'STOP' : 'AUTO'}
          </button>
        </div>
      </div>

      <div className={`distance-display ${getDistanceColor(distance)}`}>
        <span className="distance-value">
          {distance !== null && distance >= 0 ? distance.toFixed(1) : '--'}
        </span>
        <span className="distance-unit">cm</span>
      </div>

      <div className="distance-bar">
        <div
          className={`distance-fill ${getDistanceColor(distance)}`}
          style={{ width: `${getBarWidth(distance)}%` }}
        />
      </div>

      <div className="distance-scale">
        <span>0</span>
        <span>50</span>
        <span>100</span>
        <span>150+</span>
      </div>

      <div className={`distance-status ${status}`}>
        {status === 'measuring' && <span>Measuring...</span>}
        {status === 'error' && <span>Sensor unavailable</span>}
        {status === 'idle' && distance !== null && distance >= 0 && (
          <span>
            {distance < 20 ? 'Too close!' : distance < 50 ? 'Warning' : 'Safe'}
          </span>
        )}
      </div>
    </div>
  );
};
