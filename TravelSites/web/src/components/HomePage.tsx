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

export function HomePage({ onSearch }: Props) {
  const today = new Date();
  const [startDate, setStartDate] = useState(format(addDays(today, 1)));
  const [endDate, setEndDate] = useState(format(addDays(today, 3)));

  const quickCities = [
    { name: '济南', emoji: '⛩️', desc: '泉城' },
    { name: '大同', emoji: '🏯', desc: '古都' },
  ];

  const themes = [
    { name: '自然风光', emoji: '🏔️' },
    { name: '人文历史', emoji: '🏛️' },
    { name: '美食之旅', emoji: '🍜' },
    { name: '户外探险', emoji: '🧗' },
  ];

  const handleExplore = () => {
    onSearch({ startDate, endDate, preference: '' });
  };

  return (
    <div className="home-page">
      <div className="hero-section">
        <h1 className="hero-title">TravelSites</h1>
        <p className="hero-subtitle">时空驱动的旅游目的地发现平台</p>
        <p className="hero-desc">输入日期，AI 帮你穷举最适合的旅行目的地</p>
      </div>

      <div className="date-picker-card card">
        <div className="date-picker-row">
          <input
            type="date"
            value={startDate}
            onChange={(e) => setStartDate(e.target.value)}
            min={format(today)}
            className="date-input-native"
          />
          <span className="date-sep">至</span>
          <input
            type="date"
            value={endDate}
            onChange={(e) => setEndDate(e.target.value)}
            min={startDate}
            className="date-input-native"
          />
        </div>
        <button className="btn btn-primary explore-btn" onClick={handleExplore}>
          🔍 开始探索
        </button>
      </div>

      <div className="section">
        <h2 className="section-title">探索目的地</h2>
        <div className="city-grid">
          {quickCities.map((city) => (
            <button
              key={city.name}
              className="city-card"
              onClick={handleExplore}
            >
              <span className="city-emoji">{city.emoji}</span>
              <span className="city-name">{city.name}</span>
              <span className="city-desc">{city.desc}</span>
            </button>
          ))}
          <button className="city-card city-more" onClick={handleExplore}>
            <span className="city-emoji">➕</span>
            <span className="city-name">更多</span>
          </button>
        </div>
      </div>

      <div className="section">
        <h2 className="section-title">热门主题</h2>
        <div className="theme-grid">
          {themes.map((theme) => (
            <button
              key={theme.name}
              className="theme-card"
              onClick={handleExplore}
            >
              <span className="theme-emoji">{theme.emoji}</span>
              <span className="theme-name">{theme.name}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
