interface SearchInput {
  startDate?: string;
  endDate?: string;
  duration?: number;
  style?: string;
  sortBy?: string;
  preference: string;
}

interface Props {
  onSearch: (input: SearchInput, origin: { province: string; city: string; county: string }) => void;
  onOpenFilter: () => void;
  onOpenPicker: () => void;
  origin: { province: string; city: string; county: string };
  loading: boolean;
  currentSearch: {
    withDate: boolean;
    startDate?: string;
    endDate?: string;
    duration?: number;
    style?: string;
    preference: string;
  };
}

const STYLE_LABELS: Record<string, string> = {
  standard: '标准',
  family: '亲子',
  budget: '穷游',
};

export function SearchBar({ onSearch, onOpenFilter, onOpenPicker, origin, loading, currentSearch }: Props) {
  const { withDate, startDate, endDate, duration, style, preference } = currentSearch;

  const summary = (() => {
    const parts: string[] = [];
    if (withDate && startDate && endDate) {
      const s = new Date(startDate);
      const e = new Date(endDate);
      parts.push(`${s.getMonth() + 1}/${s.getDate()} - ${e.getMonth() + 1}/${e.getDate()}`);
    } else if (duration) {
      parts.push(`${duration}天`);
    }
    if (style && style !== 'standard') {
      parts.push(STYLE_LABELS[style] || style);
    }
    if (preference) {
      parts.push(preference);
    }
    return parts.join(' · ') || '点击设置搜索';
  })();

  return (
    <div className="search-bar-collapsed">
      <button type="button" className="search-bar-origin" onClick={onOpenPicker}>
        {origin.county}
      </button>

      <div className="search-bar-summary" onClick={onOpenFilter}>
        {summary}
      </div>

      <button
        type="button"
        className="search-bar-settings"
        onClick={onOpenFilter}
        title="搜索设置"
      >
        ⚙️
      </button>

      <button
        type="button"
        className="search-bar-search"
        onClick={() => onSearch(
          withDate
            ? { startDate, endDate, preference }
            : { duration, style, preference },
          origin
        )}
        disabled={loading}
        title="搜索"
      >
        {loading ? '...' : '🔍'}
      </button>
    </div>
  );
}
