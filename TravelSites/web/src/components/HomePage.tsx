import { useState } from 'react';
import { format, addDays } from '../utils/date';

interface FilterData {
  startDate: string;
  endDate: string;
  preference: string;
}

interface Props {
  onSearch: (filters: FilterData) => void;
}

const CITIES = [
  { name: '济南', image: 'jinan', tag: '泉城', desc: '72 名泉汇聚' },
  { name: '大同', image: 'datong', tag: '古都', desc: '云冈石窟' },
  { name: '青岛', image: 'ocean', tag: '海岛', desc: '红瓦绿树' },
  { name: '杭州', image: 'culture', tag: '江南', desc: '西湖十景' },
];

const THEMES = [
  { name: '自然', emoji: '🏔️', color: '#06B6D4' },
  { name: '人文', emoji: '🏛️', color: '#8B5CF6' },
  { name: '美食', emoji: '🍜', color: '#F97316' },
  { name: '户外', emoji: '🥾', color: '#10B981' },
];

// TODO 列表：未来需要扩展更多城市时，在这个数组里追加即可

export function HomePage({ onSearch }: Props) {
  const today = new Date();
  const [startDate, setStartDate] = useState(format(addDays(today, 1)));
  const [endDate, setEndDate] = useState(format(addDays(today, 3)));

  const handleExplore = (preference: string = '') => {
    onSearch({ startDate, endDate, preference });
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
        <button className="explore-btn" onClick={() => handleExplore('')}>
          立即探索 →
        </button>
      </div>

      <div className="section">
        <div className="section-header">
          <h2 className="section-title">覆盖目的地</h2>
          <span className="section-more">查看全部 ›</span>
        </div>
        <div className="city-grid">
          {CITIES.map((city) => (
            <div
              key={city.name}
              className="city-card-large"
              onClick={() => handleExplore(city.name)}
              style={{ backgroundImage: `url(/assets/${city.image}.png)` }}
            >
              <div className="city-card-overlay" />
              <div className="city-card-info">
                <span className="city-tag">{city.tag}</span>
                <span className="city-name">{city.name}</span>
                <span className="city-desc">{city.desc}</span>
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="section">
        <div className="section-header">
          <h2 className="section-title">筛选维度</h2>
        </div>
        <div className="theme-grid">
          {THEMES.map((theme) => (
            <div
              key={theme.name}
              className="theme-pill"
              onClick={() => handleExplore(theme.name)}
              style={{ '--theme-color': theme.color } as React.CSSProperties}
            >
              <span className="theme-emoji">{theme.emoji}</span>
              <span className="theme-name">{theme.name}</span>
            </div>
          ))}
        </div>
      </div>

      <div className="section">
        <div className="cta-banner">
          <div className="cta-banner-content">
            <h3 className="cta-title">按日期检索行程</h3>
            <p className="cta-desc">预设目的地 × 真实天气 × 多日方案</p>
          </div>
          <button className="cta-btn" onClick={() => handleExplore('')}>
            开始 →
          </button>
        </div>
      </div>
    </div>
  );
}