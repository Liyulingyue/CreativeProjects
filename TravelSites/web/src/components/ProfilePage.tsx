import { useEffect, useState } from 'react';
import type { Health } from '../types';

interface User {
  id: number;
  username: string;
  role: string;
  display_name: string | null;
  email: string | null;
}

interface Props {
  health: Health | null;
  user: User | null;
  seedCities: string[];
  onOpenCityManager: () => void;
  onOpenSettings: () => void;
  onLoginClick: () => void;
  onLogout: () => void;
}

export function ProfilePage({ health, user, seedCities, onOpenCityManager, onOpenSettings, onLoginClick, onLogout }: Props) {
  const [overview, setOverview] = useState<any>(null);
  const [logs, setLogs] = useState<any[]>([]);
  const [poiStatus, setPoiStatus] = useState<{ enabled: boolean; message: string } | null>(null);

  const isAdmin = user?.role === 'admin';

  useEffect(() => {
    if (!user) return;
    if (!isAdmin) {
      fetch('/api/admin/overview', {
        headers: { Authorization: `Bearer ${localStorage.getItem('travelsites_token')}` },
      })
        .then((r) => (r.ok ? r.json() : null))
        .then((d) => d && setOverview(d))
        .catch(() => {});
    } else {
      const token = localStorage.getItem('travelsites_token');
      const headers = { Authorization: `Bearer ${token}` };
      Promise.all([
        fetch('/api/admin/overview', { headers }).then((r) => r.json()),
        fetch('/api/admin/logs?limit=10', { headers }).then((r) => r.json()),
        fetch('/api/admin/poi/status', { headers }).then((r) => r.json()),
      ]).then(([ov, lgs, poi]) => {
        setOverview(ov);
        setLogs(lgs.logs || []);
        setPoiStatus(poi);
      }).catch(() => {});
    }
  }, [user, isAdmin]);

  // 未登录：引导登录
  if (!user) {
    return (
      <div className="profile-container">
        <div className="profile-hero">
          <div className="profile-avatar-large">👤</div>
          <h2>登录 TravelSites</h2>
          <p className="profile-subtitle">保存搜索历史、收藏行程、个性化推荐</p>
          <button className="btn btn-primary profile-login-btn" onClick={onLoginClick}>
            登录 / 注册
          </button>
        </div>

        <div className="profile-section">
          <div className="info-row">
            <span className="info-label">覆盖城市</span>
            <span className="info-value">{health?.cached_cities || 0} 个</span>
          </div>
          <div className="info-row">
            <span className="info-label">数据区间</span>
            <span className="info-value">
              {health?.date_range
                ? `${health.date_range[0]} ~ ${health.date_range[1]}`
                : '暂无数据'}
            </span>
          </div>
        </div>

        <div className="profile-section">
          <h3>关于</h3>
          <p className="profile-about">
            TravelSites - 时空驱动的旅游发现平台。<br />
            输入日期，AI 穷举所有时空可行的目的地。
          </p>
        </div>
      </div>
    );
  }

  // 登录后：用户信息
  return (
    <div className="profile-container">
      <div className="profile-hero">
        <div className="profile-avatar-large">
          {user.display_name?.[0] || user.username[0]}
        </div>
        <h2>{user.display_name || user.username}</h2>
        <p className="profile-subtitle">
          {user.role === 'admin' ? '👑 管理员' : '普通用户'}
          {user.email && ` · ${user.email}`}
        </p>
        <button className="btn btn-secondary profile-logout-btn" onClick={onLogout}>
          退出登录
        </button>
      </div>

      {/* 系统总览（仅管理员） */}
      {isAdmin && overview && (
        <div className="profile-section">
          <h3>系统总览</h3>
          <div className="stats-grid">
            <div className="stat-card">
              <div className="stat-num">{overview.cached_cities}</div>
              <div className="stat-label">已缓存城市</div>
            </div>
            <div className="stat-card">
              <div className="stat-num">{overview.cells_total}</div>
              <div className="stat-label">行程 cell</div>
            </div>
            <div className="stat-card">
              <div className="stat-num">{overview.cache_hit_rate.toFixed(0)}%</div>
              <div className="stat-label">缓存命中率</div>
            </div>
            <div className="stat-card">
              <div className="stat-num">{overview.generation_runs}</div>
              <div className="stat-label">生成任务</div>
            </div>
          </div>
        </div>
      )}

      {/* 管理员管理入口 */}
      {isAdmin && (
        <div className="profile-section">
          <div className="info-row">
            <span className="info-label">目标城市</span>
            <button className="manage-btn" onClick={onOpenCityManager}>
              <span className="manage-count">{seedCities.length}</span>
              管理
            </button>
          </div>
          <div className="info-row">
            <span className="info-label">系统设置</span>
            <button className="manage-btn" onClick={onOpenSettings}>
              ⚙️ 配置
            </button>
          </div>
        </div>
      )}

      {/* 管理员专属 */}
      {isAdmin && (
        <>
          <div className="profile-section">
            <h3>POI 数据源</h3>
            <div className="info-row">
              <span className="info-label">高德 POI</span>
              <span className="info-value" style={{ color: poiStatus?.enabled ? 'var(--success)' : 'var(--text-muted)' }}>
                {poiStatus?.enabled ? '● 已启用' : '○ 未启用'}
              </span>
            </div>
            {!poiStatus?.enabled && (
              <p className="profile-helper">
                在 .env 中设置 <code>AMAP_API_KEY</code> 和 <code>POI_SOURCE_ENABLED=true</code> 后重启
              </p>
            )}
          </div>

          <div className="profile-section">
            <h3>最近生成日志</h3>
            {logs.length === 0 ? (
              <p className="profile-helper">暂无记录</p>
            ) : (
              <div className="log-list">
                {logs.map((log) => (
                  <div key={log.id} className="log-row">
                    <div className="log-city">{log.city}</div>
                    <div className="log-meta">
                      {log.cells_success}/{log.cells_total} · {log.duration_seconds?.toFixed(1)}s
                    </div>
                    <div className="log-time">{log.started_at?.slice(0, 16).replace('T', ' ')}</div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </>
      )}

      <div className="profile-section">
        <h3>关于</h3>
        <p className="profile-about">
          TravelSites - 时空驱动的旅游发现平台<br />
          © 2026 · MIT License
        </p>
      </div>
    </div>
  );
}