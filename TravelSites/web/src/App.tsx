import { useState, useEffect } from 'react';
import {
  fetchHealth,
  searchTravelPlans,
} from './api/client';
import type { Health, SearchResult, SearchResultItem } from './types';
import { SearchBar } from './components/SearchBar';
import { SearchResultsList } from './components/SearchResultsList';
import { DetailModal } from './components/DetailModal';
import { Settings } from './components/Settings';

type TabType = 'home' | 'settings';

export default function App() {
  const [health, setHealth] = useState<Health | null>(null);
  const [activeTab, setActiveTab] = useState<TabType>('home');
  const [theme, setTheme] = useState<'light' | 'dark'>(() => {
    return (localStorage.getItem('theme') as 'light' | 'dark') || 'light';
  });

  const [searchResults, setSearchResults] = useState<SearchResult | null>(null);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchError, setSearchError] = useState('');
  const [selectedItem, setSelectedItem] = useState<SearchResultItem | null>(null);

  const [toast, setToast] = useState('');

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
    localStorage.setItem('theme', theme);
  }, [theme]);

  useEffect(() => {
    fetchHealth()
      .then(setHealth)
      .catch(console.warn);
  }, []);

  const showToast = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(''), 2400);
  };

  const handleSearch = async (startDate: string, endDate: string) => {
    setSearchLoading(true);
    setSearchError('');
    setSearchResults(null);

    try {
      const results = await searchTravelPlans(startDate, endDate);
      setSearchResults(results);
      if (results.total === 0) {
        showToast('未找到匹配的方案');
      }
    } catch (e) {
      setSearchError((e as Error).message);
      showToast('搜索失败');
    } finally {
      setSearchLoading(false);
    }
  };

  return (
    <div className="app">
      <header className="app-header">
        <h1>TravelSites</h1>
        <button
          className="theme-toggle"
          onClick={() => setTheme(theme === 'light' ? 'dark' : 'light')}
        >
          {theme === 'light' ? '🌙' : '☀️'}
        </button>
      </header>

      <main className="app-content">
        {activeTab === 'home' && (
          <>
            <SearchBar onSearch={handleSearch} loading={searchLoading} />

            {searchLoading && (
              <div className="loading">
                <div className="spinner" />
                <p style={{ marginTop: 12 }}>搜索中...</p>
              </div>
            )}

            {searchError && (
              <div className="card" style={{ textAlign: 'center', color: 'var(--danger)' }}>
                <p>{searchError}</p>
              </div>
            )}

            {searchResults && !searchLoading && (
              <SearchResultsList
                results={searchResults}
                onItemClick={setSelectedItem}
              />
            )}

            {!searchResults && !searchLoading && !searchError && (
              <div className="card" style={{ textAlign: 'center', color: 'var(--text-muted)', padding: '40px 20px' }}>
                <p style={{ fontSize: 48, marginBottom: 16 }}>🌍</p>
                <p>输入出发和返回日期</p>
                <p>开始探索你的下一次旅行</p>
              </div>
            )}
          </>
        )}

        {activeTab === 'settings' && (
          <Settings health={health} />
        )}
      </main>

      <nav className="bottom-nav">
        <button
          className={`bottom-nav-item ${activeTab === 'home' ? 'active' : ''}`}
          onClick={() => setActiveTab('home')}
        >
          <span className="icon">🔍</span>
          <span>搜索</span>
        </button>
        <button
          className={`bottom-nav-item ${activeTab === 'settings' ? 'active' : ''}`}
          onClick={() => setActiveTab('settings')}
        >
          <span className="icon">⚙️</span>
          <span>设置</span>
        </button>
      </nav>

      {selectedItem && (
        <DetailModal
          item={selectedItem}
          onClose={() => setSelectedItem(null)}
        />
      )}

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
