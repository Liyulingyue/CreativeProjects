import { useState, useEffect } from 'react';
import {
  fetchHealth,
  searchTravelPlans,
  getToken,
  setToken as saveToken,
  getUser,
  setUser as saveUser,
  logout as apiLogout,
} from './api/client';
import type { Health, SearchResult, SearchResultItem } from './types';
import { SearchBar } from './components/SearchBar';
import { HomePage } from './components/HomePage';
import { SearchResultsList } from './components/SearchResultsList';
import { DetailModal } from './components/DetailModal';
import { ProfilePage } from './components/ProfilePage';
import { FilterModal } from './components/FilterModal';
import { Interstitial } from './components/Interstitial';
import { LocationPicker } from './components/LocationPicker';
import { LoginModal } from './components/LoginModal';
import { CityManagerModal } from './components/CityManagerModal';
import { AdminSettingsModal } from './components/AdminSettingsModal';

type TabType = 'home' | 'search' | 'profile';

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
  const [origin, setOrigin] = useState({ province: '北京市', city: '北京市', county: '朝阳区' });
  const [showLocationPicker, setShowLocationPicker] = useState(false);
  const [lastSearchParams, setLastSearchParams] = useState<{ startDate: string; endDate: string; preference: string } | null>(null);

  // 用户态
  const [, setTokenState] = useState<string | null>(() => getToken());
  const [user, setUserState] = useState<any>(() => getUser());
  const [showLogin, setShowLogin] = useState(false);
  const [showCityManager, setShowCityManager] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [seedCities, setSeedCities] = useState<string[]>([]);

  const handleAuth = (newToken: string, newUser: any) => {
    saveToken(newToken);
    saveUser(newUser);
    setTokenState(newToken);
    setUserState(newUser);
    showToast(`欢迎，${newUser.display_name || newUser.username}！`);
  };

  const handleLogout = async () => {
    try {
      await apiLogout();
    } catch (e) {
      // ignore
    }
    saveToken(null);
    saveUser(null);
    setTokenState(null);
    setUserState(null);
    showToast('已退出登录');
  };

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

  useEffect(() => {
    if (activeTab === 'profile' && user?.role === 'admin') {
      fetch('/api/admin/cities', {
        headers: { Authorization: `Bearer ${localStorage.getItem('travelsites_token')}` },
      })
        .then((r) => r.json())
        .then((d) => setSeedCities(d.cities || []))
        .catch(() => {});
    }
  }, [activeTab, user]);

  const showToast = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(''), 2400);
  };

  const doSearch = async (startDate: string, endDate: string, preference: string = '', origin: { province: string; city: string; county: string } = { province: '北京市', city: '北京市', county: '朝阳区' }) => {
    setSearchLoading(true);
    setSearchError('');
    setLastSearchParams({ startDate, endDate, preference });

    try {
      const results = await searchTravelPlans(startDate, endDate, preference, origin);
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

  const handleOriginPicked = (picked: { province: string; city: string; county: string }) => {
    setOrigin(picked);
    if (lastSearchParams) {
      doSearch(
        lastSearchParams.startDate,
        lastSearchParams.endDate,
        lastSearchParams.preference,
        picked
      );
      showToast(`出发地已更新为 ${picked.county}`);
    }
  };

  const handleSearch = async (startDate: string, endDate: string, origin: { province: string; city: string; county: string }) => {
    await doSearch(startDate, endDate, '', origin);
    setActiveTab('search');
  };

  const handleFilterApply = async (filters: FilterData) => {
    await doSearch(filters.startDate, filters.endDate, filters.preference, origin);
    setActiveTab('search');
  };

  return (
    <div className="app">
      <header className="app-header">
        <h1>TravelSites</h1>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          {user ? (
            <>
              <span style={{ fontSize: 13, color: 'rgba(255,255,255,0.8)' }}>
                {user.username}{user.role === 'admin' && ' 👑'}
              </span>
              <button className="user-button" onClick={handleLogout}>
                退出
              </button>
            </>
          ) : (
            <button className="user-button" onClick={() => setShowLogin(true)}>
              👤 登录
            </button>
          )}
          <button
            className="theme-toggle"
            onClick={() => setTheme(theme === 'light' ? 'dark' : 'light')}
          >
            {theme === 'light' ? '🌙' : '☀️'}
          </button>
        </div>
      </header>

      {activeTab === 'home' && (
        <HomePage onSearch={handleFilterApply} seedCities={seedCities} />
      )}

      {activeTab === 'search' && (
        <>
          <div className="search-bar-wrapper">
            <SearchBar
              onSearch={handleSearch}
              onExpand={() => setShowFilter(true)}
              onOpenPicker={() => setShowLocationPicker(true)}
              origin={origin}
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

      {activeTab === 'profile' && (
        <main className="app-content">
          <ProfilePage
            health={health}
            user={user}
            seedCities={seedCities}
            onSeedCitiesChange={setSeedCities}
            onOpenCityManager={() => setShowCityManager(true)}
            onOpenSettings={() => setShowSettings(true)}
            onLoginClick={() => setShowLogin(true)}
            onLogout={handleLogout}
          />
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
          className={`bottom-nav-item ${activeTab === 'profile' ? 'active' : ''}`}
          onClick={() => setActiveTab('profile')}
        >
          <span className="icon">👤</span>
          <span>我的</span>
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

      {showLogin && (
        <LoginModal
          onClose={() => setShowLogin(false)}
          onAuth={handleAuth}
        />
      )}

      {showLocationPicker && (
        <LocationPicker
          onClose={() => setShowLocationPicker(false)}
          onConfirm={handleOriginPicked}
          current={origin}
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

      {showCityManager && (
        <CityManagerModal
          cities={seedCities}
          onSave={(updated) => {
            setSeedCities(updated);
            setShowCityManager(false);
          }}
          onClose={() => setShowCityManager(false)}
        />
      )}

      {showSettings && (
        <AdminSettingsModal onClose={() => setShowSettings(false)} />
      )}

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
