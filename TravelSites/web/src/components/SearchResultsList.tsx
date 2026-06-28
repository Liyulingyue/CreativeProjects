import type { SearchResult, SearchResultItem } from '../types';
import { formatDisplayDate } from '../utils/date';

interface Props {
  results: SearchResult;
  onItemClick: (item: SearchResultItem) => void;
}

function getScoreColor(score: number): string {
  if (score >= 85) return '#10B981';
  if (score >= 70) return '#0D9488';
  if (score >= 50) return '#F59E0B';
  return '#EF4444';
}

export function SearchResultsList({ results, onItemClick }: Props) {
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

  return (
    <div>
      <div className="results-header">
        找到 <strong>{results.total}</strong> 个推荐目的地
      </div>

      {results.items.map((item) => (
        <div
          key={`${item.city}-${item.start_date}-${item.end_date}`}
          className="result-card"
          onClick={() => onItemClick(item)}
        >
          <div className="result-card-row">
            <div className="result-left">
              <div className="result-city-line">
                <span className="city-name">{item.city}</span>
                <span className="result-duration">{item.duration_days}天</span>
              </div>
              <div className="result-meta">
                <span className="date-range">
                  {formatDisplayDate(item.start_date)} - {formatDisplayDate(item.end_date)}
                </span>
                <span
                  className="rec-badge"
                  style={{ background: getScoreColor(item.score) }}
                >
                  {item.recommendation}
                </span>
              </div>
              {item.key_highlights && (
                <p className="result-highlights">{item.key_highlights}</p>
              )}
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
