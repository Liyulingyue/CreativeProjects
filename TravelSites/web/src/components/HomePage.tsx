import { useMemo, useState } from 'react';
import { format, addDays, formatShort } from '../utils/date';
import { dailyShuffle } from '../utils/random';

interface SearchInput {
  startDate?: string;
  endDate?: string;
  duration?: number;
  style?: string;
  sortBy?: string;
  preference: string;
}

interface Props {
  onSearch: (input: SearchInput) => void;
  seedCities: string[];
}

const THEMES = [
  { name: '自然风光', color: '#06B6D4', image: 'mountain' },
  { name: '人文历史', color: '#8B5CF6', image: 'culture' },
  { name: '美食', color: '#F97316', image: 'food' },
  { name: '亲子', color: '#EC4899', image: 'forest' },
  { name: '户外探险', color: '#10B981', image: 'hiking' },
  { name: '网红打卡', color: '#EF4444', image: 'sunset' },
  { name: '海岛', color: '#0EA5E9', image: 'beach' },
  { name: '古镇', color: '#A16207', image: 'culture' },
  { name: '沙漠', color: '#D97706', image: 'sunset' },
  { name: '雪山', color: '#64748B', image: 'snow' },
  { name: '温泉', color: '#E11D48', image: 'forest' },
  { name: '夜景', color: '#1E40AF', image: 'nightcity' },
];

const DURATIONS = [1, 2, 3, 4, 5];
const STYLES = [
  { key: 'standard', label: '标准' },
  { key: 'family', label: '亲子' },
  { key: 'budget', label: '穷游' },
];

function calcDurationDays(start: string, end: string): number {
  const s = new Date(start);
  const e = new Date(end);
  const diff = Math.ceil((e.getTime() - s.getTime()) / (1000 * 60 * 60 * 24));
  return Math.max(1, diff);
}

export function HomePage({ onSearch, seedCities }: Props) {
  const today = new Date();
  const [showDatePicker, setShowDatePicker] = useState(false);
  const [startDate, setStartDate] = useState(format(addDays(today, 1)));
  const [endDate, setEndDate] = useState(format(addDays(today, 3)));
  const [duration, setDuration] = useState(2);
  const [style, setStyle] = useState('standard');

  const cards = useMemo(() => {
    const cities = dailyShuffle(seedCities, Math.min(6, seedCities.length));
    const themes = dailyShuffle(THEMES, Math.min(6, cities.length || 6));
    return cities.map((city, i) => ({
      city,
      theme: themes[i % themes.length],
    }));
  }, [seedCities]);

  const dateChipText = showDatePicker
    ? `${formatShort(startDate)} - ${formatShort(endDate)}`
    : '近期出发';

  const handleStartDateChange = (value: string) => {
    setStartDate(value);
    const newEnd = format(addDays(new Date(value), duration));
    if (newEnd >= value) {
      setEndDate(newEnd);
    }
  };

  const handleEndDateChange = (value: string) => {
    setEndDate(value);
    if (value >= startDate) {
      setDuration(calcDurationDays(startDate, value));
    }
  };

  const handleDurationChange = (d: number) => {
    setDuration(d);
    if (showDatePicker) {
      setEndDate(format(addDays(new Date(startDate), d)));
    }
  };

  const buildSearchInput = (preference: string): SearchInput => {
    const base = { duration, style, preference };
    if (showDatePicker) {
      return { startDate, endDate, ...base };
    }
    return base;
  };

  const handleExplore = (city: string, themeName: string) => {
    const preference = [city, themeName].filter(Boolean).join('，');
    onSearch(buildSearchInput(preference));
  };

  const handleSearch = () => {
    onSearch(buildSearchInput(''));
  };

  return (
    <div className="home-page">
      <div className="hero-section">
        <div className="hero-bg-shape hero-bg-shape-1" />
        <div className="hero-bg-shape hero-bg-shape-2" />

        <img
          src="/assets/logo.png"
          alt="TravelSites"
          className="hero-logo"
          onError={(e) => { (e.target as HTMLImageElement).style.display = 'none'; }}
        />
        <h1 className="hero-title">TravelSites</h1>
        <p className="hero-subtitle">时空驱动的旅游搜索</p>
        <p className="hero-desc">目的地 · 日期 · 天数 · 风格</p>
      </div>

      <div className="date-picker-card">
        <div className="quick-filters-row">
          <div className="seg-group">
            {DURATIONS.map((d) => (
              <button
                key={d}
                type="button"
                className={`seg-btn ${duration === d ? 'active' : ''}`}
                onClick={() => handleDurationChange(d)}
              >{d}天</button>
            ))}
          </div>
          <div className="seg-group" style={{ marginLeft: 8 }}>
            {STYLES.map((s) => (
              <button
                key={s.key}
                type="button"
                className={`seg-btn ${style === s.key ? 'active' : ''}`}
                onClick={() => setStyle(s.key)}
              >{s.label}</button>
            ))}
          </div>
        </div>

        <button
          type="button"
          className={`date-chip ${showDatePicker ? 'active' : ''}`}
          onClick={() => setShowDatePicker((v) => !v)}
        >
          <span className="date-chip-icon">📅</span>
          <span className="date-chip-text">{dateChipText}</span>
          <span className="date-chip-hint">{showDatePicker ? '收起' : '选择日期'}</span>
        </button>

        {showDatePicker && (
          <div className="date-picker-row">
            <div className="date-field">
              <span className="date-label">出发</span>
              <input
                type="date"
                value={startDate}
                onChange={(e) => handleStartDateChange(e.target.value)}
                min={format(today)}
              />
            </div>
            <div className="date-arrow">→</div>
            <div className="date-field">
              <span className="date-label">返回</span>
              <input
                type="date"
                value={endDate}
                onChange={(e) => handleEndDateChange(e.target.value)}
                min={startDate}
              />
            </div>
          </div>
        )}

        <button className="explore-btn" onClick={handleSearch}>
          探索 →
        </button>
      </div>

      <div className="section">
        <div className="section-header">
          <h2 className="section-title">今日推荐</h2>
          <span className="section-hint">点击直达搜索</span>
        </div>
        <div className="combo-grid">
          {cards.map(({ city, theme }) => (
            <div
              key={city}
              className="combo-card"
              onClick={() => handleExplore(city, theme.name)}
              title={`${city} · ${theme.name}`}
            >
              <div
                className="combo-image"
                style={{ backgroundImage: `url(/assets/${theme.image}.png)` }}
              >
                <div className="combo-image-overlay" />
                <div className="combo-caption">
                  <div className="combo-theme-tag" style={{ background: theme.color }}>
                    {theme.name}
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
