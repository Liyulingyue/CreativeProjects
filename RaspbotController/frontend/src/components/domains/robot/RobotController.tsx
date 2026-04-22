import { useState, useCallback, useEffect, useRef } from 'react';
import { carApi } from '../../../api';
import type { Direction } from '../../../api';
import './index.css';

const CONTROLS: { direction: Direction; label: string; className: string }[] = [
  { direction: 'forward', label: '▲', className: 'btn-up' },
  { direction: 'left', label: '◀', className: 'btn-left' },
  { direction: 'stop', label: '■', className: 'btn-center' },
  { direction: 'right', label: '▶', className: 'btn-right' },
  { direction: 'backward', label: '▼', className: 'btn-down' },
];

const KEY_MAP: Record<string, Direction> = {
  w: 'forward', s: 'backward', a: 'spin_left', d: 'spin_right',
  W: 'forward', S: 'backward', A: 'spin_left', D: 'spin_right',
  ArrowUp: 'forward', ArrowDown: 'backward',
  ArrowLeft: 'spin_left', ArrowRight: 'spin_right',
  ' ': 'stop',
};

export const RobotController = () => {
  const [speed, setSpeed] = useState(50);
  const [activeDir, setActiveDir] = useState<Direction | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const handleMove = useCallback(async (direction: Direction) => {
    setActiveDir(direction);
    try {
      await carApi.move({ direction, speed });
    } catch (err) {
      console.error('[RobotController] move error:', err);
    }
  }, [speed]);

  const handleStop = useCallback(async () => {
    setActiveDir(null);
    try {
      await carApi.stop();
    } catch (err) {
      console.error('[RobotController] stop error:', err);
    }
  }, []);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const dir = KEY_MAP[e.key];
      if (!dir) return;
      e.preventDefault();
      handleMove(dir);
    };
    const onKeyUp = (e: KeyboardEvent) => {
      if (KEY_MAP[e.key]) {
        e.preventDefault();
        handleStop();
      }
    };

    const el = containerRef.current;
    if (el) {
      el.addEventListener('keydown', onKeyDown);
      el.addEventListener('keyup', onKeyUp);
      el.setAttribute('tabindex', '0');
      el.focus();
    }
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);

    return () => {
      if (el) {
        el.removeEventListener('keydown', onKeyDown);
        el.removeEventListener('keyup', onKeyUp);
      }
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
    };
  }, [handleMove, handleStop]);

  return (
    <div className="controller-container" ref={containerRef}>
      <div className="control-panel">
        <div className="speed-control">
          <label>Speed: {speed}%</label>
          <input type="range" min="10" max="100" value={speed} onChange={e => setSpeed(Number(e.target.value))} />
        </div>

        <div className="control-grid">
          {CONTROLS.map(c => (
            <button
              key={c.direction}
              className={`control-btn ${c.className} ${activeDir === c.direction ? 'active' : ''}`}
              onMouseDown={() => handleMove(c.direction)}
              onMouseUp={handleStop}
              onMouseLeave={handleStop}
            >
              {c.label}
            </button>
          ))}
        </div>

        <div className="spin-control">
          <button className={`control-btn ${activeDir === 'left' ? 'active' : ''}`}
            onMouseDown={() => handleMove('left')} onMouseUp={handleStop} onMouseLeave={handleStop}>◀ Spin</button>
          <button className={`control-btn ${activeDir === 'right' ? 'active' : ''}`}
            onMouseDown={() => handleMove('right')} onMouseUp={handleStop} onMouseLeave={handleStop}>Spin ▶</button>
        </div>

        <div className="keyboard-hint">
          <span>Click this panel first, then use: W/S/A/D or Arrow Keys</span>
        </div>
      </div>
    </div>
  );
};
