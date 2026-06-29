import { useMemo, useState } from 'react';
import { format, addDays } from '../utils/date';
import { dailyShuffle } from '../utils/random';

interface FilterData {
  startDate: string;
  endDate: string;
  preference: string;
}

interface Props {
  onSearch: (filters: FilterData) => void;
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

export function HomePage({ onSearch, seedCities }: Props) {
  const today = new Date();
  const [startDate, setStartDate] = useState(format(addDays(today, 1)));
  const [endDate, setEndDate] = useState(format(addDays(today, 3)));

  // 6 个城市 + 6 个主题的组合
  const cards = useMemo(() => {
    const cities = dailyShuffle(seedCities, Math.min(6, seedCities.length));
    const themes = dailyShuffle(THEMES, Math.min(6, cities.length || 6));
    return cities.map((city, i) => ({
      city,
      theme: themes[i % themes.length],
    }));
  }, [seedCities]);

  const handleExplore = (city: string, themeName: string) => {
    onSearch({
      startDate,
      endDate,
      preference: [city, themeName].filter(Boolean).join('，'),
    });
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
        <p className="hero-desc">输入日期 · 检索匹配目的地 · 快速查看行程</p>
      </div>

      <div className="date-picker-card">
        <div className="date-picker-row">
          <div className="date-field">
            <span className="date-label">出发</span>
            <input
              type="date"
              value={startDate}
              onChange={(e) => setStartDate(e.target.value)}
              min={format(today)}
            />
          </div>
          <div className="date-arrow">→</div>
          <div className="date-field">
            <span className="date-label">返回</span>
            <input
              type="date"
              value={endDate}
              onChange={(e) => setEndDate(e.target.value)}
              min={startDate}
            />
          </div>
        </div>
        <button className="explore-btn" onClick={() => handleExplore('', '')}>
          立即探索 →
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
