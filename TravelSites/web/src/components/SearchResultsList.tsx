import type { SearchResult, SearchResultItem } from '../types';
import { formatDisplayDate } from '../utils/date';

interface Props {
  results: SearchResult;
  onItemClick: (item: SearchResultItem) => void;
}

function getScoreColor(score: number): string {
  if (score >= 85) return '#52c41a';
  if (score >= 70) return '#4A90E2';
  if (score >= 50) return '#faad14';
  return '#f5222d';
}

function getRecBadgeColor(rec: string): string {
  if (rec === '强烈推荐') return '#52c41a';
  if (rec === '推荐') return '#4A90E2';
  if (rec === '勉强可行') return '#faad14';
  if (rec === '建议改期') return '#f5222d';
  return '#999';
}

export function SearchResultsList({ results, onItemClick }: Props) {
  if (results.total === 0) {
    return (
      <div className="card" style={{ textAlign: 'center', color: 'var(--text-muted)', padding: 40 }}>
        <p style={{ fontSize: 40, marginBottom: 12 }}>😢</p>
        <p>暂未找到匹配的方案</p>
        <p style={{ fontSize: 13, marginTop: 8 }}>试试调整日期范围</p>
      </div>
    );
  }

  return (
    <div className="results-list">
      <div className="results-header">
        <span>找到 <strong>{results.total}</strong> 个推荐目的地</span>
      </div>

      {results.items.map((item) => (
        <div
          key={`${item.city}-${item.start_date}-${item.end_date}`}
          className="result-card card"
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
                  style={{ background: getRecBadgeColor(item.recommendation) }}
                >
                  {item.recommendation}
                </span>
              </div>
              <p className="result-highlights">{item.key_highlights}</p>
            </div>
            <div className="result-score" style={{ color: getScoreColor(item.score) }}>
              <span className="score-num">{item.score}</span>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
