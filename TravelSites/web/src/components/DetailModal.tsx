import type { SearchResultItem, DailyPlan } from '../types';

interface Props {
  item: SearchResultItem;
  onClose: () => void;
}

function getScoreColor(score: number): string {
  if (score >= 85) return '#52c41a';
  if (score >= 70) return '#4A90E2';
  if (score >= 50) return '#faad14';
  return '#f5222d';
}

function formatDate(dateStr: string): string {
  const d = new Date(dateStr);
  return `${d.getMonth() + 1}月${d.getDate()}日`;
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

function DaySection({ day }: { day: DailyPlan }) {
  return (
    <div className="day-section">
      <div className="day-header">
        <span className="day-badge">第{day.day}天</span>
        <span className="day-date">{formatDate(day.date)}</span>
        <span className="day-theme">{day.theme}</span>
      </div>
      {day.weather_hint && (
        <div className="weather-hint">💡 {day.weather_hint}</div>
      )}
      {day.routes.map((route) => (
        <div key={route.route_id} className="route-section">
          <div className="route-tags">
            {route.tags.map((tag) => (
              <span key={tag} className="route-tag">{tag}</span>
            ))}
            <span className="route-hours">⏱️ {route.total_hours}h</span>
          </div>
          <div className="activities">
            {route.activities.map((act, i) => (
              <div key={i} className="activity-item">
                <TimeSlotIcon slot={act.time_slot} />
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
  );
}

export function DetailModal({ item, onClose }: Props) {
  const breakdown = item.score_breakdown;
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
              <span>{formatDate(item.start_date)} - {formatDate(item.end_date)}</span>
            </div>
            <div className="detail-row">
              <span className="detail-label">天数</span>
              <span>{item.duration_days} 天</span>
            </div>
            <div className="detail-row">
              <span className="detail-label">推荐</span>
              <span style={{ color: getScoreColor(item.score), fontWeight: 600 }}>
                {item.recommendation}
              </span>
            </div>
          </div>

          <div className="detail-section">
            <h3>综合评分</h3>
            <div style={{ display: 'flex', alignItems: 'baseline', gap: 8 }}>
              <span style={{ fontSize: 48, fontWeight: 700, color: getScoreColor(item.score) }}>
                {item.score}
              </span>
              <span style={{ fontSize: 16, color: getScoreColor(item.score) }}>/100</span>
            </div>
          </div>

          {breakdown && (
            <div className="detail-section">
              <h3>评分明细</h3>
              <div style={{ display: 'grid', gap: 10 }}>
                {[
                  { label: '天数匹配度', value: breakdown.days_match, icon: '📅' },
                  { label: '天气友好度', value: breakdown.weather, icon: '🌤️' },
                  { label: '景点丰富度', value: breakdown.attraction_density, icon: '🏞️' },
                  { label: '交通便利度', value: breakdown.transport, icon: '🚄' },
                ].map((item) => (
                  <div key={item.label} style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                    <span style={{ fontSize: 18 }}>{item.icon}</span>
                    <div style={{ flex: 1 }}>
                      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 13, marginBottom: 3 }}>
                        <span>{item.label}</span>
                        <span style={{ fontWeight: 600 }}>{item.value}</span>
                      </div>
                      <div className="score-bar">
                        <div
                          className="score-bar-fill"
                          style={{
                            width: `${item.value}%`,
                            background: getScoreColor(item.value)
                          }}
                        />
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {item.weather_summary && (
            <div className="detail-section">
              <h3>天气预报</h3>
              <p>{item.weather_summary}</p>
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
              {item.daily_plan.map((day) => (
                <DaySection key={day.day} day={day} />
              ))}
            </div>
          )}

          {item.key_highlights && !hasDailyPlan && (
            <div className="detail-section">
              <h3>行程亮点</h3>
              <p>{item.key_highlights}</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
