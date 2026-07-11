import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getStats } from "@/api/settings";
import type { Stats } from "@/api/types";

export function Dashboard() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getStats()
      .then(setStats)
      .catch((e) => setError(e.message));
  }, []);

  if (error) {
    return (
      <div className="page">
        <h1>概览</h1>
        <div className="error-card">
          <p>无法连接后端服务</p>
          <p className="error-card__detail">{error}</p>
          <p>请确保后端服务已启动，且 <code>/api</code> 接口可用</p>
        </div>
      </div>
    );
  }

  return (
    <div className="page">
      <h1>概览</h1>
      <div className="stats-grid">
        <div className="stat-card">
          <div className="stat-card__value">{stats?.total_photos ?? "—"}</div>
          <div className="stat-card__label">照片总数</div>
        </div>
        <div className="stat-card">
          <div className="stat-card__value">{stats?.analyzed_photos ?? "—"}</div>
          <div className="stat-card__label">已分析</div>
        </div>
        <div className="stat-card">
          <div className="stat-card__value">{stats?.duplicate_groups ?? "—"}</div>
          <div className="stat-card__label">重复组</div>
        </div>
        <div className="stat-card">
          <div className="stat-card__value">{stats?.directories ?? "—"}</div>
          <div className="stat-card__label">目录</div>
        </div>
      </div>

      <div className="quick-actions">
        <h2>快捷操作</h2>
        <div className="action-grid">
          <Link to="/explorer" className="action-card">
            <span className="action-card__icon">📁</span>
            <span className="action-card__label">浏览照片</span>
          </Link>
          <Link to="/analysis" className="action-card">
            <span className="action-card__icon">🔍</span>
            <span className="action-card__label">分析照片</span>
          </Link>
          <Link to="/dedup" className="action-card">
            <span className="action-card__icon">⊞</span>
            <span className="action-card__label">照片去重</span>
          </Link>
          <Link to="/settings" className="action-card">
            <span className="action-card__icon">⚙</span>
            <span className="action-card__label">系统设置</span>
          </Link>
        </div>
      </div>
    </div>
  );
}
