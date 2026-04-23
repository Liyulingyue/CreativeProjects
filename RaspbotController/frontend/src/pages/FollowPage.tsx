import { useState, useCallback, useEffect } from 'react';
import { carApi } from '../api';
import './FollowPage.css';

interface FollowConfig {
  zone_near: number;
  zone_hold_max: number;
  zone_approach: number;
  zone_far: number;
  speed: number;
  scan_interval: number;
}

export const FollowPage = () => {
  const [following, setFollowing] = useState(false);
  const [loading, setLoading] = useState(false);
  const [distance, setDistance] = useState<number>(-1);
  const [zone, setZone] = useState('IDLE');
  const [config, setConfig] = useState<FollowConfig>({
    zone_near: 20,
    zone_hold_max: 50,
    zone_approach: 100,
    zone_far: 120,
    speed: 35,
    scan_interval: 3,
  });
  const [cfgEditing, setCfgEditing] = useState(false);
  const [cfgSaving, setCfgSaving] = useState(false);

  const loadConfig = useCallback(async () => {
    try {
      const cfg = await carApi.followConfig() as unknown as FollowConfig;
      setConfig(cfg);
    } catch {
      // use defaults
    }
  }, []);

  const fetchStatus = useCallback(async () => {
    try {
      const result = await carApi.followStatus();
      setFollowing(result.status === 'running');
      if (result.distance !== undefined) {
        setDistance(result.distance);
        if (result.zone) setZone(result.zone);
      }
    } catch {
      setFollowing(false);
    }
  }, []);

  useEffect(() => {
    loadConfig();
    fetchStatus();
    const interval = setInterval(fetchStatus, 300);
    return () => clearInterval(interval);
  }, [fetchStatus, loadConfig]);

  const handleToggle = useCallback(async () => {
    setLoading(true);
    try {
      if (following) {
        await carApi.followStop();
        setFollowing(false);
      } else {
        await carApi.followStart();
        setFollowing(true);
      }
    } catch (err) {
      console.error('[FollowPage] error:', err);
    } finally {
      setLoading(false);
    }
  }, [following]);

  const handleCfgSave = useCallback(async () => {
    setCfgSaving(true);
    try {
      const updated = await carApi.followConfigUpdate(config) as unknown as FollowConfig;
      setConfig({ ...updated });
      setCfgEditing(false);
    } catch (err) {
      console.error('[FollowPage] config save error:', err);
    } finally {
      setCfgSaving(false);
    }
  }, [config]);

  const zoneLegend = [
    { label: '超过远距离阈值', desc: '不反应', cls: 'far' },
    { label: '接近阈值到远距离阈值之间', desc: '快速接近', cls: 'approach' },
    { label: '过近阈值到跟随上限之间', desc: '保持距离', cls: 'hold' },
    { label: '低于过近阈值', desc: '后退', cls: 'near' },
  ];

  return (
    <div className="page">
      <div className="page-header">
        <h2>Auto Follow</h2>
        <p className="page-desc">Zone-based autonomous tracking</p>
      </div>

      <div className="follow-content">
        <div className="follow-camera">
          <div className="camera-wrapper">
            <img src="/api/v1/camera/stream" alt="Camera Feed" />
            {following && <div className="follow-overlay"><span className="tracking-badge">TRACKING</span></div>}
            {distance > 0 && (
              <div className="distance-badge">{distance.toFixed(0)}cm</div>
            )}
            <div className="follow-overlay">
              <span className="tracking-badge" style={{background: following ? 'linear-gradient(135deg, #00e676, #00c853)' : '#666'}}>
                {following ? 'TRACKING' : '待机'}
              </span>
            </div>
          </div>
        </div>

        <div className="follow-controls">
          <div className="follow-info">
            <h3>区间控制</h3>
            <div className="zone-legend">
              {zoneLegend.map(z => (
                <span key={z.cls} className="zone-item"><b>{z.label}</b> {z.desc}</span>
              ))}
            </div>
            <div className="zone-values">
              <span>远距离阈值: <b>{config.zone_far}cm</b></span>
              <span>接近阈值: <b>{config.zone_approach}cm</b></span>
              <span>跟随上限: <b>{config.zone_hold_max}cm</b></span>
              <span>过近阈值: <b>{config.zone_near}cm</b></span>
            </div>
            <p className="zone-current">状态: <span className={`zone-badge zone-${zone.toLowerCase()}`}>
              {zone === 'FAR' ? '远距离' : zone === 'APPROACH' ? '接近中' : zone === 'HOLD' ? '跟随中' : zone === 'NEAR' ? '过近' : zone === 'LOST' ? '丢失' : '待机'}
            </span></p>

            <div className="cfg-form">
              <label>过近阈值 (cm) <input type="number" disabled={!cfgEditing} value={config.zone_near} onChange={e => setConfig(p => ({ ...p, zone_near: Number(e.target.value) }))} /></label>
              <label>跟随上限 (cm) <input type="number" disabled={!cfgEditing} value={config.zone_hold_max} onChange={e => setConfig(p => ({ ...p, zone_hold_max: Number(e.target.value) }))} /></label>
              <label>接近阈值 (cm) <input type="number" disabled={!cfgEditing} value={config.zone_approach} onChange={e => setConfig(p => ({ ...p, zone_approach: Number(e.target.value) }))} /></label>
              <label>远距离阈值 (cm) <input type="number" disabled={!cfgEditing} value={config.zone_far} onChange={e => setConfig(p => ({ ...p, zone_far: Number(e.target.value) }))} /></label>
              <label>速度 <input type="number" min="10" max="100" disabled={!cfgEditing} value={config.speed} onChange={e => setConfig(p => ({ ...p, speed: Number(e.target.value) }))} /></label>
              <label>扫描间隔 (秒) <input type="number" min="1" max="10" disabled={!cfgEditing} value={config.scan_interval} onChange={e => setConfig(p => ({ ...p, scan_interval: Number(e.target.value) }))} /></label>
              <div className="cfg-actions">
                {!cfgEditing ? (
                  <button className="cfg-edit-btn" onClick={() => setCfgEditing(true)}>编辑配置</button>
                ) : (
                  <>
                    <button onClick={handleCfgSave} disabled={cfgSaving}>{cfgSaving ? '保存中...' : '保存'}</button>
                    <button onClick={() => { setCfgEditing(false); loadConfig(); }} disabled={cfgSaving}>取消</button>
                  </>
                )}
              </div>
            </div>
          </div>

          <button className={`follow-btn ${following ? 'active' : ''}`} onClick={handleToggle} disabled={loading}>
            {loading ? '...' : following ? '停止跟随' : '开始跟随'}
          </button>

          <div className={`follow-status ${following ? 'running' : ''}`}>
            <span className="status-dot" />
            <span>{following ? '运行中 - 追踪目标' : '待机'}</span>
          </div>
        </div>
      </div>
    </div>
  );
};
