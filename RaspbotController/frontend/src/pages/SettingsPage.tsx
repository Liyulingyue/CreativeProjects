import { useState } from 'react';
import './Settings.css';

export const SettingsPage = () => {
  const [settings, setSettings] = useState({
    serverUrl: 'http://192.168.2.198:8000',
    cameraQuality: 80,
    cameraInterval: 300,
    servo1Default: 90,
    servo2Default: 90,
    servo3Default: 90,
    buzzerFreq: 440,
  });

  const handleChange = (key: string, value: string | number) => {
    setSettings(prev => ({ ...prev, [key]: value }));
  };

  return (
    <div className="page">
      <div className="page-header">
        <h2>Settings</h2>
        <p className="page-desc">Configure system parameters</p>
      </div>

      <div className="settings-grid">
        <div className="settings-section">
          <h3>Connection</h3>
          <div className="setting-item">
            <label>Server URL</label>
            <input
              type="text"
              value={settings.serverUrl}
              onChange={e => handleChange('serverUrl', e.target.value)}
              className="setting-input"
            />
          </div>
        </div>

        <div className="settings-section">
          <h3>Camera</h3>
          <div className="setting-item">
            <label>Quality: {settings.cameraQuality}%</label>
            <input
              type="range"
              min="30"
              max="100"
              value={settings.cameraQuality}
              onChange={e => handleChange('cameraQuality', Number(e.target.value))}
              className="setting-slider"
            />
          </div>
          <div className="setting-item">
            <label>Refresh Interval: {settings.cameraInterval}ms</label>
            <input
              type="range"
              min="100"
              max="1000"
              step="50"
              value={settings.cameraInterval}
              onChange={e => handleChange('cameraInterval', Number(e.target.value))}
              className="setting-slider"
            />
          </div>
        </div>

        <div className="settings-section">
          <h3>Servo Defaults</h3>
          <div className="setting-item">
            <label>Camera H: {settings.servo1Default}°</label>
            <input
              type="range"
              min="0"
              max="180"
              value={settings.servo1Default}
              onChange={e => handleChange('servo1Default', Number(e.target.value))}
              className="setting-slider"
            />
          </div>
          <div className="setting-item">
            <label>Camera V: {settings.servo2Default}°</label>
            <input
              type="range"
              min="0"
              max="180"
              value={settings.servo2Default}
              onChange={e => handleChange('servo2Default', Number(e.target.value))}
              className="setting-slider"
            />
          </div>
          <div className="setting-item">
            <label>Arm: {settings.servo3Default}°</label>
            <input
              type="range"
              min="0"
              max="180"
              value={settings.servo3Default}
              onChange={e => handleChange('servo3Default', Number(e.target.value))}
              className="setting-slider"
            />
          </div>
        </div>

        <div className="settings-section">
          <h3>Buzzer</h3>
          <div className="setting-item">
            <label>Default Frequency: {settings.buzzerFreq} Hz</label>
            <input
              type="range"
              min="100"
              max="1000"
              value={settings.buzzerFreq}
              onChange={e => handleChange('buzzerFreq', Number(e.target.value))}
              className="setting-slider"
            />
          </div>
        </div>
      </div>

      <div className="settings-footer">
        <button className="save-btn" onClick={() => alert('Settings saved!')}>
          Save Settings
        </button>
      </div>
    </div>
  );
};
