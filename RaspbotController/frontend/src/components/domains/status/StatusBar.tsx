import { useHealthCheck } from '../../../hooks/useHealthCheck';
import './index.css';

export const StatusBar = () => {
  const health = useHealthCheck();

  return (
    <div className="status-bar">
      <div className="status-item">
        <span className={`dot ${health?.car ? 'green' : 'red'}`}></span>
        <span>Car {health?.car ? 'Ready' : 'Offline'}</span>
      </div>
      <div className="status-item">
        <span className={`dot ${health?.camera ? 'green' : 'red'}`}></span>
        <span>Camera {health?.camera ? 'Ready' : 'Offline'}</span>
      </div>
    </div>
  );
};
