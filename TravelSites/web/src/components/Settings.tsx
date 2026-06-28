import type { Health } from '../types';

interface Props {
  health: Health | null;
}

export function Settings({ health }: Props) {
  return (
    <div style={{ padding: '0 0 90px 0' }}>
      <div className="settings-section">
        <h3 style={{ fontSize: 13, fontWeight: 700, color: 'var(--text-muted)', marginBottom: 12, textTransform: 'uppercase', letterSpacing: '0.5px' }}>
          系统状态
        </h3>
        <div className="settings-row">
          <span className="settings-label">服务状态</span>
          <span className="settings-value" style={{ color: health?.status === 'ok' ? 'var(--success)' : 'var(--danger)' }}>
            {health?.status === 'ok' ? '正常' : '异常'}
          </span>
        </div>
        <div className="settings-row">
          <span className="settings-label">种子城市</span>
          <span className="settings-value">{health?.seed_cities?.join('、') || '-'}</span>
        </div>
        <div className="settings-row">
          <span className="settings-label">定时刷新</span>
          <span className="settings-value" style={{ color: health?.refresh_enabled ? 'var(--success)' : 'var(--text-muted)' }}>
            {health?.refresh_enabled ? '已启用' : '已禁用'}
          </span>
        </div>
      </div>

      <div className="settings-section">
        <h3 style={{ fontSize: 13, fontWeight: 700, color: 'var(--text-muted)', marginBottom: 12, textTransform: 'uppercase', letterSpacing: '0.5px' }}>
          使用说明
        </h3>
        <div style={{ fontSize: 13, color: 'var(--text-secondary)', lineHeight: 1.8 }}>
          <p>1. 选择城市查看不同出发日和天数的旅行方案矩阵</p>
          <p>2. 矩阵中数字为综合评分，点击可查看详情</p>
          <p>3. 评分基于天数匹配度、天气、景点丰富度、交通便利度计算</p>
          <p>4. 后台会自动/定时生成最新数据</p>
        </div>
      </div>

      <div className="settings-section">
        <h3 style={{ fontSize: 13, fontWeight: 700, color: 'var(--text-muted)', marginBottom: 12, textTransform: 'uppercase', letterSpacing: '0.5px' }}>
          关于
        </h3>
        <div style={{ fontSize: 13, color: 'var(--text-secondary)', lineHeight: 1.8 }}>
          <p style={{ fontWeight: 600, color: 'var(--text)', marginBottom: 4 }}>TravelSites</p>
          <p>时空驱动的旅游目的地发现平台</p>
          <p style={{ marginTop: 8 }}>
            把"先决定去哪"的漏斗反转，由 AI 穷举所有时空可行的目的地候选。
          </p>
        </div>
      </div>
    </div>
  );
}
