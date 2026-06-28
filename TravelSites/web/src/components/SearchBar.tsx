import { useState } from 'react';
import { format, addDays, formatShort } from '../utils/date';

interface Props {
  onSearch: (startDate: string, endDate: string, origin: string) => void;
  onExpand: () => void;
  onOpenPicker: () => void;
  origin: string;
  loading: boolean;
}

export function SearchBar({ onSearch, onExpand, onOpenPicker, origin, loading }: Props) {
  const today = new Date();
  const defaultStart = format(addDays(today, 1));
  const defaultEnd = format(addDays(today, 3));

  const [startDate, setStartDate] = useState(defaultStart);
  const [endDate, setEndDate] = useState(defaultEnd);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!startDate || !endDate) return;
    onSearch(startDate, endDate, origin);
  };

  return (
    <form className="search-bar-compact" onSubmit={handleSubmit}>
      <button
        type="button"
        className="origin-display"
        onClick={onOpenPicker}
      >
        {origin}
      </button>

      <div className="date-sep-v" />

      <div className="date-group">
        <div className="date-display-cell" onClick={(e) => {
          const target = e.currentTarget.querySelector('input') as HTMLInputElement | null;
          target?.showPicker?.() || target?.focus();
        }}>
          <input
            type="date"
            value={startDate}
            onChange={(e) => setStartDate(e.target.value)}
            min={format(today)}
            className="date-input-hidden"
          />
          <span className="date-display-text">{formatShort(startDate)}</span>
        </div>

        <span className="date-sep">-</span>

        <div className="date-display-cell" onClick={(e) => {
          const target = e.currentTarget.querySelector('input') as HTMLInputElement | null;
          target?.showPicker?.() || target?.focus();
        }}>
          <input
            type="date"
            value={endDate}
            onChange={(e) => setEndDate(e.target.value)}
            min={startDate}
            className="date-input-hidden"
          />
          <span className="date-display-text">{formatShort(endDate)}</span>
        </div>
      </div>

      <button type="submit" className="btn-search" disabled={loading}>
        {loading ? '...' : '🔍'}
      </button>
      <button type="button" className="btn-expand" onClick={onExpand}>
        ⚙️
      </button>
    </form>
  );
}