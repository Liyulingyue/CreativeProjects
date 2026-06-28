import type { Health } from '../types';

interface Props {
  health: Health | null;
}

export function Settings({ health }: Props) {
  return (
    <div>
      <div className="card">
        <div className="card-title">系统状态</div>
        <div style={{ display: 'grid', gap: 8, fontSize: 14 }}>
          <div style={{ display: 'flex', justifyContent: 'space-between' }}>
            <span style={{ color: 'var(--text-secondary)' }}>服务状态</span>
            <span style={{ color: health?.status === 'ok' ? '#52c41a' : '#f5222d' }}>
              {health?.status === 'ok' ? '正常' : '异常'}
            </span>
          </div>
          <div style={{ display: 'flex', justifyContent: 'space-between' }}>
            <span style={{ color: 'var(--text-secondary)' }}>种子城市</span>
            <span>{health?.seed_cities?.join(', ') || '-'}</span>
          </div>
          <div style={{ display: 'flex', justifyContent: 'space-between' }}>
            <span style={{ color: 'var(--text-secondary)' }}>定时刷新</span>
            <span style={{ color: health?.refresh_enabled ? '#52c41a' : 'var(--text-muted)' }}>
              {health?.refresh_enabled ? '已启用' : '已禁用'}
            </span>
          </div>
        </div>
      </div>

      <div className="card">
        <div className="card-title">使用说明</div>
        <div style={{ fontSize: 13, color: 'var(--text-secondary)', lineHeight: 1.8 }}>
          <p>1. 选择城市查看该城市不同出发日和天数的旅行方案矩阵</p>
          <p>2. 矩阵中数字为综合评分，点击可查看详情</p>
          <p>3. 评分基于天数匹配度、天气、景点丰富度、交通便利度计算</p>
          <p>4. 后台会自动/定时生成最新数据</p>
        </div>
      </div>

      <div className="card">
        <div className="card-title">关于</div>
        <div style={{ fontSize: 13, color: 'var(--text-secondary)' }}>
          <p>TravelSites - 时空驱动的旅游目的地发现平台</p>
          <p style={{ marginTop: 8 }}>把"先决定去哪"的漏斗反转，由 AI 穷举所有时空可行的目的地候选。</p>
        </div>
      </div>
    </div>
  );
}
