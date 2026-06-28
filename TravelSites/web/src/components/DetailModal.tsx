import type { SearchResultItem } from '../types';

interface Props {
  item: SearchResultItem;
  onClose: () => void;
}

function getScoreColor(score: number): string {
  if (score >= 85) return '#10B981';
  if (score >= 70) return '#0D9488';
  if (score >= 50) return '#F59E0B';
  return '#EF4444';
}

function formatDate(dateStr: string): string {
  const d = new Date(dateStr);
  return `${d.getMonth() + 1}月${d.getDate()}日`;
}

function ScoreCircle({ score }: { score: number }) {
  const radius = 42;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference - (score / 100) * circumference;
  const color = getScoreColor(score);

  return (
    <div className="score-display">
      <div className="score-circle">
        <svg width="100" height="100" viewBox="0 0 100 100">
          <circle
            className="score-circle-bg"
            cx="50" cy="50" r={radius}
          />
          <circle
            className="score-circle-fill"
            cx="50" cy="50" r={radius}
            stroke={color}
            strokeDasharray={circumference}
            strokeDashoffset={offset}
          />
        </svg>
        <div className="score-circle-text">
          <span className="score-circle-num" style={{ color }}>{score}</span>
        </div>
      </div>
    </div>
  );
}

function TimeSlotIcon({ slot }: { slot: string }) {
  const icons: Record<string, string> = {
    '上午': '🌅',
    '中午': '☀️',
    '下午': '🌤️',
    '晚上': '🌙',
  };
  return <span>{icons[slot] || '📍'}</span>;
}

export function DetailModal({ item, onClose }: Props) {
  const breakdown = item.score_breakdown as Record<string, number>;
  const hasDailyPlan = item.daily_plan && item.daily_plan.length > 0;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>{item.city}</h2>
          <button className="modal-close" onClick={onClose}>×</button>
        </div>

        <div className="modal-body">
          <div className="detail-section">
            <h3>行程概览</h3>
            <div className="detail-row">
              <span className="detail-label">日期</span>
              <span className="detail-value">{formatDate(item.start_date)} - {formatDate(item.end_date)}</span>
            </div>
            <div className="detail-row">
              <span className="detail-label">天数</span>
              <span className="detail-value">{item.duration_days} 天</span>
            </div>
            {item.distance_km != null && item.distance_km > 0 && (
              <div className="detail-row">
                <span className="detail-label">距出发地</span>
                <span className="detail-value">
                  {Math.round(item.distance_km)} km
                  {item.transit_hours != null && item.transit_hours > 0 && (
                    <span style={{ color: 'var(--text-muted)', fontWeight: 400, marginLeft: 6 }}>
                      · 预估 {item.transit_hours}h
                    </span>
                  )}
                </span>
              </div>
            )}
            <div className="detail-row">
              <span className="detail-label">推荐</span>
              <span className="detail-value" style={{ color: getScoreColor(item.score) }}>
                {item.recommendation}
              </span>
            </div>
          </div>

          <ScoreCircle score={item.score} />

          {breakdown && Object.keys(breakdown).length > 0 && (
            <div className="detail-section">
              <h3>评分明细</h3>
              {[
                { label: '天数匹配度', value: breakdown.days_match, icon: '📅' },
                { label: '天气友好度', value: breakdown.weather, icon: '🌤️' },
                { label: '景点丰富度', value: breakdown.attraction_density, icon: '🏞️' },
                { label: '交通便利度', value: breakdown.transport, icon: '🚄' },
              ].map((item) => (
                <div key={item.label} style={{ marginBottom: 8 }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 13, marginBottom: 3 }}>
                    <span style={{ color: 'var(--text-secondary)' }}>{item.icon} {item.label}</span>
                    <span style={{ fontWeight: 700 }}>{item.value}</span>
                  </div>
                  <div className="score-bar">
                    <div
                      className="score-bar-fill"
                      style={{ width: `${item.value}%`, background: getScoreColor(item.value) }}
                    />
                  </div>
                </div>
              ))}
            </div>
          )}

          {item.weather_summary && (
            <div className="detail-section">
              <h3>天气预报</h3>
              <p style={{ fontSize: 14, color: 'var(--text-secondary)', lineHeight: 1.6 }}>
                {item.weather_summary}
              </p>
            </div>
          )}

          {item.top_attractions.length > 0 && (
            <div className="detail-section">
              <h3>推荐景点</h3>
              <div className="attraction-list">
                {item.top_attractions.map((a, i) => (
                  <div key={i} className="attraction-item">
                    <span className="attraction-num">{i + 1}</span>
                    <span>{a}</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {hasDailyPlan && (
            <div className="detail-section">
              <h3>每日行程</h3>
              {item.daily_plan.map((day: any) => (
                <div key={day.day} className="day-section">
                  <div className="day-header">
                    <span className="day-badge">第{day.day}天</span>
                    <span className="day-date">{formatDate(day.date)}</span>
                    {day.theme && <span className="day-theme">{day.theme}</span>}
                  </div>
                  {day.weather_hint && (
                    <div className="weather-hint">💡 {day.weather_hint}</div>
                  )}
                  {day.routes && day.routes.map((route: any) => (
                    <div key={route.route_id} className="route-section">
                      <div className="route-tags">
                        {route.tags && route.tags.map((tag: string) => (
                          <span key={tag} className="route-tag">{tag}</span>
                        ))}
                        {route.total_hours && (
                          <span className="route-hours">⏱️ {route.total_hours}h</span>
                        )}
                      </div>
                      <div className="activities">
                        {route.activities && route.activities.map((act: any, i: number) => (
                          <div key={i} className="activity-item">
                            <div className="activity-icon">
                              <TimeSlotIcon slot={act.time_slot} />
                            </div>
                            <div className="activity-content">
                              <span className="activity-name">{act.attraction}</span>
                              <span className="activity-meta">{act.time_slot} · {act.hours}h</span>
                              {act.notes && <p className="activity-notes">{act.notes}</p>}
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
