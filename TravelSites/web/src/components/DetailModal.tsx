import { useEffect, useState } from 'react';
import type { SearchResultItem } from '../types';

interface Props {
  item: SearchResultItem;
  onClose: () => void;
}

interface AttractionDetail {
  name: string;
  category: string | null;
  rating: number | null;
  suggested_hours: number | null;
  address: string | null;
  tags: string[];
}

interface HolidayInsight {
  crowd_level: string;
  activity_level: string;
  price_multiplier: number;
  tips: string[];
  holidays: Array<{ date: string; name: string; type: string }>;
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

function crowdEmoji(level: string): string {
  if (level === 'extreme') return '🔥';
  if (level === 'high') return '👥';
  if (level === 'medium') return '🚶';
  return '🌿';
}

function crowdLabel(level: string): string {
  if (level === 'extreme') return '人流极旺';
  if (level === 'high') return '人流较高';
  if (level === 'medium') return '人流适中';
  return '人流较少';
}

export function DetailModal({ item, onClose }: Props) {
  const breakdown = item.score_breakdown as Record<string, number>;
  const hasDailyPlan = item.daily_plan && item.daily_plan.length > 0;

  const [attractionDetails, setAttractionDetails] = useState<Record<string, AttractionDetail>>({});
  const [holidayInsight, setHolidayInsight] = useState<HolidayInsight | null>(null);

  useEffect(() => {
    // 拉取景点详情
    if (item.top_attractions && item.top_attractions.length > 0) {
      Promise.all(
        item.top_attractions.map(async (name) => {
          const url = `/api/attractions/search?q=${encodeURIComponent(name)}&city=${encodeURIComponent(item.city)}&limit=1`;
          try {
            const res = await fetch(url);
            const data = await res.json();
            if (data.results && data.results.length > 0) {
              // 再查详情拿 rating
              const detailRes = await fetch(`/api/attractions?city=${encodeURIComponent(item.city)}&limit=20`);
              const detailData = await detailRes.json();
              const full = detailData.items?.find((a: any) => a.name === name);
              return [name, full || data.results[0]];
            }
            return [name, null];
          } catch {
            return [name, null];
          }
        })
      ).then((pairs) => {
        const map: Record<string, AttractionDetail> = {};
        pairs.forEach(([name, detail]: any) => {
          if (detail) {
            map[name] = {
              name: detail.name,
              category: detail.category,
              rating: detail.rating,
              suggested_hours: detail.suggested_hours,
              address: detail.address,
              tags: detail.tags || [],
            };
          }
        });
        setAttractionDetails(map);
      });
    }

    // 拉取节假日洞察
    fetch(`/api/holidays?start_date=${item.start_date}&end_date=${item.end_date}`)
      .then((res) => res.json())
      .then((data) => setHolidayInsight(data))
      .catch(() => {});
  }, [item.city, item.start_date, item.end_date]);

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

          {holidayInsight && holidayInsight.holidays && holidayInsight.holidays.length > 0 && (
            <div className="detail-section">
              <h3>节假日出行</h3>
              <div className="holiday-tags">
                <span className="holiday-tag">
                  {crowdEmoji(holidayInsight.crowd_level)} {crowdLabel(holidayInsight.crowd_level)}
                </span>
                {holidayInsight.price_multiplier > 1.0 && (
                  <span className="holiday-tag warning">
                    💰 价格 ×{holidayInsight.price_multiplier.toFixed(2)}
                  </span>
                )}
                {holidayInsight.holidays.map((h, i) => (
                  <span key={i} className="holiday-tag info">
                    🎉 {formatDate(h.date)} {h.name}
                  </span>
                ))}
              </div>
              {holidayInsight.tips.length > 0 && (
                <div className="holiday-tips">
                  {holidayInsight.tips.map((t, i) => (
                    <div key={i} className="holiday-tip">💡 {t}</div>
                  ))}
                </div>
              )}
            </div>
          )}

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
                      style={{
                        width: `${item.value}%`,
                        background: getScoreColor(item.value)
                      }}
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
                {item.top_attractions.map((name, i) => {
                  const detail = attractionDetails[name];
                  return (
                    <div key={i} className="attraction-item">
                      <span className="attraction-num">{i + 1}</span>
                      <div style={{ flex: 1 }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
                          <span style={{ fontWeight: 600 }}>{name}</span>
                          {detail?.category && (
                            <span className="attraction-cat-tag">{detail.category}</span>
                          )}
                          {detail?.rating != null && (
                            <span className="attraction-rating">⭐ {detail.rating.toFixed(1)}</span>
                          )}
                          {detail?.suggested_hours != null && (
                            <span className="attraction-hours">⏱️ {detail.suggested_hours}h</span>
                          )}
                        </div>
                        {detail?.tags && detail.tags.length > 0 && (
                          <div className="attraction-mini-tags">
                            {detail.tags.map((t) => (
                              <span key={t} className="attraction-mini-tag">{t}</span>
                            ))}
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}
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
                              <span className="activity-name">
                                {act.attraction}
                                {act.verified === true && (
                                  <span className="verified-badge" title="数据库验证">✓</span>
                                )}
                                {act.verified === false && (
                                  <span className="unverified-badge" title="AI 推荐">~</span>
                                )}
                              </span>
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