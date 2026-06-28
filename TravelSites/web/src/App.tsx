import { useState, useEffect } from 'react';
import {
  fetchHealth,
  searchTravelPlans,
} from './api/client';
import type { Health, SearchResult, SearchResultItem } from './types';
import { SearchBar } from './components/SearchBar';
import { HomePage } from './components/HomePage';
import { SearchResultsList } from './components/SearchResultsList';
import { DetailModal } from './components/DetailModal';
import { Settings } from './components/Settings';
import { FilterModal } from './components/FilterModal';
import { Interstitial } from './components/Interstitial';

type TabType = 'home' | 'search' | 'settings';

interface FilterData {
  startDate: string;
  endDate: string;
  preference: string;
}

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
  const [showFilter, setShowFilter] = useState(false);
  const [showInterstitial, setShowInterstitial] = useState(true);

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

  const doSearch = async (startDate: string, endDate: string, preference: string = '') => {
    setSearchLoading(true);
    setSearchError('');

    try {
      const results = await searchTravelPlans(startDate, endDate, preference);
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

  const handleSearch = async (startDate: string, endDate: string) => {
    await doSearch(startDate, endDate, '');
    setActiveTab('search');
  };

  const handleFilterApply = async (filters: FilterData) => {
    await doSearch(filters.startDate, filters.endDate, filters.preference);
    setActiveTab('search');
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

      {activeTab === 'home' && (
        <HomePage onSearch={handleFilterApply} />
      )}

      {activeTab === 'search' && (
        <>
          <div className="search-bar-wrapper">
            <SearchBar
              onSearch={handleSearch}
              onExpand={() => setShowFilter(true)}
              loading={searchLoading}
            />
          </div>
          <main className="app-content">
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
          </main>
        </>
      )}

      {activeTab === 'settings' && (
        <main className="app-content">
          <Settings health={health} />
        </main>
      )}

      <nav className="bottom-nav">
        <button
          className={`bottom-nav-item ${activeTab === 'home' ? 'active' : ''}`}
          onClick={() => setActiveTab('home')}
        >
          <span className="icon">🏠</span>
          <span>首页</span>
        </button>
        <button
          className={`bottom-nav-item ${activeTab === 'search' ? 'active' : ''}`}
          onClick={() => setActiveTab('search')}
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

      {showFilter && (
        <FilterModal
          onClose={() => setShowFilter(false)}
          onApply={handleFilterApply}
        />
      )}

      {showInterstitial && (
        <Interstitial
          onClose={() => setShowInterstitial(false)}
          onEnter={() => {
            setShowInterstitial(false);
            setActiveTab('search');
          }}
        />
      )}

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
