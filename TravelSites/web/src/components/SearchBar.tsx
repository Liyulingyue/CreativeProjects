import { useState } from 'react';
import { format, addDays } from '../utils/date';

interface Props {
  onSearch: (startDate: string, endDate: string) => void;
  onExpand: () => void;
  loading: boolean;
}

export function SearchBar({ onSearch, onExpand, loading }: Props) {
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
    <form className="search-bar-compact" onSubmit={handleSubmit}>
      <input
        type="date"
        value={startDate}
        onChange={(e) => setStartDate(e.target.value)}
        min={format(today)}
        className="date-input-native"
      />
      <span className="date-sep">-</span>
      <input
        type="date"
        value={endDate}
        onChange={(e) => setEndDate(e.target.value)}
        min={startDate}
        className="date-input-native"
      />
      <button type="submit" className="btn-search" disabled={loading}>
        {loading ? '...' : '🔍'}
      </button>
      <button type="button" className="btn-expand" onClick={onExpand}>
        ⚙️
      </button>
    </form>
  );
}
