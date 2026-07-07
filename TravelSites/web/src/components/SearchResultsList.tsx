import { useState, useMemo } from 'react';
import type { SearchResult, SearchResultItem } from '../types';
import { formatDisplayDate } from '../utils/date';

interface Props {
  results: SearchResult;
  onItemClick: (item: SearchResultItem) => void;
}

const SORT_OPTIONS = [
  { key: 'score', label: '综合评分' },
  { key: 'distance', label: '距离最近' },
  { key: 'weather', label: '天气最优' },
  { key: 'preference', label: '偏好匹配' },
];

function getScoreColor(score: number): string {
  if (score >= 85) return '#10B981';
  if (score >= 70) return '#0D9488';
  if (score >= 50) return '#F59E0B';
  return '#EF4444';
}

export function SearchResultsList({ results, onItemClick }: Props) {
  const [sortBy, setSortBy] = useState('score');

  const sortedItems = useMemo(() => {
    const items = [...results.items];
    switch (sortBy) {
      case 'distance':
        return items.sort((a, b) => (a.distance_km || 0) - (b.distance_km || 0));
      case 'weather':
        return items.sort((a, b) => {
          const aHasWeather = a.weather_summary ? 1 : 0;
          const bHasWeather = b.weather_summary ? 1 : 0;
          return bHasWeather - aHasWeather;
        });
      case 'preference':
        return items.sort((a, b) => (b.preference_score || 0) - (a.preference_score || 0));
      default:
        return items.sort((a, b) => b.score - a.score);
    }
  }, [results.items, sortBy]);

  if (results.total === 0) {
    return (
      <div className="empty-state">
        <img
          src="/assets/empty3.png"
          alt="无结果"
          className="empty-illustration"
          onError={(e) => { (e.target as HTMLImageElement).style.display = 'none'; }}
        />
        <h3 className="empty-title">暂无匹配的行程</h3>
        <p className="empty-desc">试试调整日期范围或筛选条件</p>
      </div>
    );
  }

  const isGuide = results.source === 'guide';

  return (
    <div>
      <div className="results-header">
        <div className="results-count">
          找到 <strong>{results.total}</strong> 个推荐目的地
          {isGuide && <span className="source-badge">攻略</span>}
          {!isGuide && <span className="source-badge source-matrix">实时</span>}
        </div>
        <div className="results-sort">
          <span className="sort-label">排序：</span>
          <div className="sort-options">
            {SORT_OPTIONS.map((opt) => (
              <button
                key={opt.key}
                className={`sort-btn ${sortBy === opt.key ? 'active' : ''}`}
                onClick={() => setSortBy(opt.key)}
              >
                {opt.label}
              </button>
            ))}
          </div>
        </div>
      </div>

      {sortedItems.map((item) => (
        <div
          key={`${item.city}-${item.start_date || ''}-${item.duration_days}`}
          className="result-card"
          onClick={() => onItemClick(item)}
        >
          <div className="result-card-row">
            <div className="result-left">
              <div className="result-city-line">
                <span className="city-name">{item.city}</span>
                <span className="result-duration">{item.duration_days}天</span>
                {item.distance_km && item.distance_km > 0 && (
                  <span className="result-distance">{item.distance_km}km</span>
                )}
              </div>
              <div className="result-meta">
                {item.start_date ? (
                  <span className="date-range">
                    {formatDisplayDate(item.start_date)} - {formatDisplayDate(item.end_date || '')}
                  </span>
                ) : (
                  <span className="date-range">{item.duration_days}天</span>
                )}
                {item.tags && item.tags.length > 0 && (
                  <span className="city-tags">
                    {item.tags.slice(0, 3).map((t) => `${t} `)}
                  </span>
                )}
                {item.recommendation && (
                  <span className="rec-badge" style={{ background: getScoreColor(item.score) }}>
                    {item.recommendation}
                  </span>
                )}
              </div>
              {item.blurb ? (
                <p className="result-highlights">{item.blurb}</p>
              ) : item.key_highlights ? (
                <p className="result-highlights">{item.key_highlights}</p>
              ) : null}
            </div>
            <div className="result-score">
              <span className="score-num" style={{ color: getScoreColor(item.score) }}>
                {item.score}
              </span>
              <span className="score-label">分</span>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
