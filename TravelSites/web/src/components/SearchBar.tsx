import { useState } from 'react';
import { format, addDays } from '../utils/date';

interface Props {
  onSearch: (startDate: string, endDate: string) => void;
  loading: boolean;
}

export function SearchBar({ onSearch, loading }: Props) {
  const today = new Date();
  const defaultStart = format(addDays(today, 1));
  const defaultEnd = format(addDays(today, 3));

  const [startDate, setStartDate] = useState(defaultStart);
  const [endDate, setEndDate] = useState(defaultEnd);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!startDate || !endDate) return;
    onSearch(startDate, endDate);
  };

  return (
    <form className="search-bar card" onSubmit={handleSubmit}>
      <div className="search-row">
        <div className="search-field">
          <label>出发</label>
          <input
            type="date"
            value={startDate}
            onChange={(e) => setStartDate(e.target.value)}
            min={format(today)}
          />
        </div>
        <div className="search-arrow">→</div>
        <div className="search-field">
          <label>返回</label>
          <input
            type="date"
            value={endDate}
            onChange={(e) => setEndDate(e.target.value)}
            min={startDate}
          />
        </div>
      </div>
      <button
        type="submit"
        className="btn btn-primary search-btn"
        disabled={loading || !startDate || !endDate}
      >
        {loading ? '搜索中...' : '🔍 搜索目的地'}
      </button>
    </form>
  );
}
