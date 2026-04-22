import { useState, useCallback } from 'react';
import { carApi } from '../../../api';
import './index.css';

export const LEDControl = () => {
  const [led1On, setLed1On] = useState(false);
  const [led2On, setLed2On] = useState(false);

  const sendLed = useCallback(async (action: string, led: number) => {
    try {
      await carApi.led(action, led);
      if (action === 'on') {
        if (led === 1) setLed1On(true);
        if (led === 2) setLed2On(true);
      } else if (action === 'off') {
        if (led === 1) setLed1On(false);
        if (led === 2) setLed2On(false);
      }
    } catch (err) {
      console.error('[LEDControl] error:', err);
    }
  }, []);

  const toggleLed = useCallback(async (led: number) => {
    try {
      await carApi.led('toggle', led);
      if (led === 1) setLed1On(prev => !prev);
      if (led === 2) setLed2On(prev => !prev);
    } catch (err) {
      console.error('[LEDControl] toggle error:', err);
    }
  }, []);

  const blinkLed = useCallback(async (led: number) => {
    try {
      await carApi.led('blink', led);
    } catch (err) {
      console.error('[LEDControl] blink error:', err);
    }
  }, []);

  return (
    <div className="led-container">
      <div className="led-header">
        <h3>LED</h3>
      </div>
      <div className="led-channels">
        <div className={`led-channel ${led1On ? 'on' : 'off'}`}>
          <div className="led-indicator red" />
          <div className="led-info">
            <span className="led-label">LED 1</span>
            <span className="led-status">{led1On ? 'ON' : 'OFF'}</span>
          </div>
          <div className="led-actions">
            <button className="led-btn on" onClick={() => sendLed('on', 1)}>ON</button>
            <button className="led-btn off" onClick={() => sendLed('off', 1)}>OFF</button>
            <button className="led-btn toggle" onClick={() => toggleLed(1)}>T</button>
            <button className="led-btn blink" onClick={() => blinkLed(1)}>B</button>
          </div>
        </div>

        <div className={`led-channel ${led2On ? 'on' : 'off'}`}>
          <div className="led-indicator blue" />
          <div className="led-info">
            <span className="led-label">LED 2</span>
            <span className="led-status">{led2On ? 'ON' : 'OFF'}</span>
          </div>
          <div className="led-actions">
            <button className="led-btn on" onClick={() => sendLed('on', 2)}>ON</button>
            <button className="led-btn off" onClick={() => sendLed('off', 2)}>OFF</button>
            <button className="led-btn toggle" onClick={() => toggleLed(2)}>T</button>
            <button className="led-btn blink" onClick={() => blinkLed(2)}>B</button>
          </div>
        </div>
      </div>
    </div>
  );
};
