import { useHealthCheck } from '../hooks/useHealthCheck';
import { StatusBar } from '../components/domains/status';
import './Dashboard.css';

export const DashboardPage = () => {
  const health = useHealthCheck();

  return (
    <div className="page">
      <div className="page-header">
        <h2>Dashboard</h2>
        <p className="page-desc">System overview and diagnostics</p>
      </div>

      <StatusBar />

      <div className="dashboard-grid">
        <div className="dashboard-card info-card">
          <h3>System Info</h3>
          <div className="info-grid">
            <div className="info-item">
              <span className="info-label">Status</span>
              <span className={`info-value ${health ? 'online' : 'offline'}`}>
                {health ? 'Online' : 'Offline'}
              </span>
            </div>
            <div className="info-item">
              <span className="info-label">Car Module</span>
              <span className={`info-value ${health?.car ? 'online' : 'offline'}`}>
                {health?.car ? 'Ready' : 'Unavailable'}
              </span>
            </div>
            <div className="info-item">
              <span className="info-label">Camera Module</span>
              <span className={`info-value ${health?.camera ? 'online' : 'offline'}`}>
                {health?.camera ? 'Ready' : 'Unavailable'}
              </span>
            </div>
            <div className="info-item">
              <span className="info-label">Buzzer Module</span>
              <span className="info-value online">Ready</span>
            </div>
          </div>
        </div>

        <div className="dashboard-card quick-actions">
          <h3>Quick Actions</h3>
          <div className="actions-list">
            <a href="#/remote" className="action-item">
              <span className="action-icon">🎮</span>
              <span className="action-text">Open Remote Control</span>
            </a>
            <a href="#/settings" className="action-item">
              <span className="action-icon">⚙️</span>
              <span className="action-text">Configure Settings</span>
            </a>
          </div>
        </div>

        <div className="dashboard-card log-card">
          <h3>Activity Log</h3>
          <div className="log-list">
            <div className="log-entry info">
              <span className="log-time">{new Date().toLocaleTimeString()}</span>
              <span className="log-msg">Dashboard loaded</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
