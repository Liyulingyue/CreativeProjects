import { useState } from 'react';
import { format, addDays, formatShort } from '../utils/date';

interface SearchInput {
  startDate?: string;
  endDate?: string;
  duration?: number;
  style?: string;
  preference: string;
}

interface Props {
  onSearch: (input: SearchInput, origin: { province: string; city: string; county: string }) => void;
  onExpand: () => void;
  onOpenPicker: () => void;
  origin: { province: string; city: string; county: string };
  loading: boolean;
}

const DURATIONS = [1, 2, 3, 4, 5];
const STYLES = [
  { key: 'standard', label: '标准' },
  { key: 'family', label: '亲子' },
  { key: 'budget', label: '穷游' },
];

export function SearchBar({ onSearch, onExpand, onOpenPicker, origin, loading }: Props) {
  const today = new Date();
  const [withDate, setWithDate] = useState(true);
  const [startDate, setStartDate] = useState(format(addDays(today, 1)));
  const [endDate, setEndDate] = useState(format(addDays(today, 3)));
  const [duration, setDuration] = useState(3);
  const [style, setStyle] = useState('standard');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (withDate) {
      onSearch({ startDate, endDate, preference: '' }, origin);
    } else {
      onSearch({ duration, style, preference: '' }, origin);
    }
  };

  return (
    <form className="search-bar-compact" onSubmit={handleSubmit}>
      <button type="button" className="origin-display" onClick={onOpenPicker}>
        {origin.county}
      </button>

      <div className="date-sep-v" />

      {withDate ? (
        <>
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
          <button
            type="button"
            className="date-toggle-btn"
            onClick={() => setWithDate(false)}
            title="切换为按天数搜索"
          >
            📅
          </button>
        </>
      ) : (
        <div className="duration-style-row-inline">
          <div className="seg-group">
            {DURATIONS.map((d) => (
              <button key={d} type="button" className={`seg-btn ${duration === d ? 'active' : ''}`} onClick={() => setDuration(d)}>
                {d}天
              </button>
            ))}
          </div>
          <div className="seg-group">
            {STYLES.map((s) => (
              <button key={s.key} type="button" className={`seg-btn ${style === s.key ? 'active' : ''}`} onClick={() => setStyle(s.key)}>
                {s.label}
              </button>
            ))}
          </div>
          <button
            type="button"
            className="date-toggle-btn"
            onClick={() => setWithDate(true)}
            title="切换为按日期搜索"
          >
            📅
          </button>
        </div>
      )}

      <button type="submit" className="btn-search" disabled={loading}>
        {loading ? '...' : '🔍'}
      </button>
      <button type="button" className="btn-expand" onClick={onExpand}>
        ⚙️
      </button>
    </form>
  );
}
