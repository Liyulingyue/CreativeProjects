import { useState, useCallback } from 'react';
import { carApi } from '../../../api';
import './index.css';

interface ServoChannelProps {
  id: number;
  name: string;
}

export const ServoControl = () => {
  const [angles, setAngles] = useState<Record<number, number>>({ 1: 90, 2: 90 });
  const [activeChannel, setActiveChannel] = useState<number | null>(null);

  const channels: ServoChannelProps[] = [
    { id: 1, name: 'Camera H' },
    { id: 2, name: 'Camera V' },
  ];

  const handleAngleChange = useCallback((id: number, angle: number) => {
    setAngles(prev => ({ ...prev, [id]: angle }));
  }, []);

  const handleSliderChange = useCallback((id: number, angle: number) => {
    handleAngleChange(id, angle);
    setActiveChannel(id);
  }, [handleAngleChange]);

  const handleSliderCommit = useCallback(async (id: number, angle: number) => {
    setActiveChannel(null);
    try {
      await carApi.servo({ id, angle });
    } catch (err) {
      console.error('[ServoControl] servo error:', err);
    }
  }, []);

  const handleReset = useCallback(async () => {
    const resetAngle = 90;
    setAngles({ 1: resetAngle, 2: resetAngle });
    try {
      await Promise.all([
        carApi.servo({ id: 1, angle: resetAngle }),
        carApi.servo({ id: 2, angle: resetAngle }),
      ]);
    } catch (err) {
      console.error('[ServoControl] reset error:', err);
    }
  }, []);

  return (
    <div className="servo-container">
      <div className="servo-header">
        <h3>Servo Control</h3>
        <button className="reset-btn" onClick={handleReset}>Reset</button>
      </div>
      <div className="servo-channels">
        {channels.map(ch => (
          <div key={ch.id} className={`servo-channel ${activeChannel === ch.id ? 'active' : ''}`}>
            <div className="channel-header">
              <span className="channel-name">{ch.name}</span>
              <span className="channel-angle">{angles[ch.id]}°</span>
            </div>
            <input
              type="range"
              min="0"
              max="180"
              value={angles[ch.id]}
              onChange={e => handleSliderChange(ch.id, Number(e.target.value))}
              onMouseUp={e => handleSliderCommit(ch.id, Number((e.target as HTMLInputElement).value))}
              onTouchEnd={e => {
                const target = e.target as HTMLInputElement;
                handleSliderCommit(ch.id, Number(target.value));
              }}
              className="servo-slider"
            />
          </div>
        ))}
      </div>
    </div>
  );
};
