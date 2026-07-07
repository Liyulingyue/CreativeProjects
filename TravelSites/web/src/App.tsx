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

interface GuideDaySchedule {
  time: string;
  spot: string;
  duration_min: number;
  note?: string;
}

interface GuideMeal {
  place: string;
  price: number;
  specialty: string;
}

interface GuideDailyPlan {
  day: number;
  theme: string;
  schedule: GuideDaySchedule[];
  meals?: {
    breakfast?: GuideMeal;
    lunch?: GuideMeal;
    dinner?: GuideMeal;
  };
  transport?: string;
  accommodation?: string;
}

function adaptGuideDailyPlan(guideDays: GuideDailyPlan[]): any[] {
  return guideDays.map((gd) => {
    const activities = gd.schedule.map((s) => ({
      attraction: s.spot,
      time_slot: s.time,
      hours: s.duration_min / 60,
      notes: s.note || '',
      verified: undefined as undefined,
    }));

    const extraNotes: string[] = [];
    if (gd.meals?.breakfast) extraNotes.push(`早: ${gd.meals.breakfast.place} (${gd.meals.breakfast.specialty})`);
    if (gd.meals?.lunch) extraNotes.push(`午: ${gd.meals.lunch.place} (${gd.meals.lunch.specialty})`);
    if (gd.meals?.dinner) extraNotes.push(`晚: ${gd.meals.dinner.place} (${gd.meals.dinner.specialty})`);
    if (gd.transport) extraNotes.push(`交通: ${gd.transport}`);
    if (gd.accommodation) extraNotes.push(`住宿: ${gd.accommodation}`);

    const combinedNotes = extraNotes.join('\n');

    return {
      day: gd.day,
      date: '',
      theme: gd.theme,
      weather_hint: '',
      routes: [
        {
          route_id: String(gd.day),
          tags: [],
          activities,
          total_hours: Math.round(gd.schedule.reduce((sum, s) => sum + s.duration_min, 0) / 60 * 10) / 10,
        },
        ...(combinedNotes ? [{
          route_id: `info-${gd.day}`,
          tags: ['信息'],
          activities: [{
            attraction: '行程信息',
            time_slot: '',
            hours: 0,
            notes: combinedNotes,
            verified: undefined,
          }],
          total_hours: 0,
        }] : []),
      ],
    };
  });
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

  // 当前搜索配置（由 SearchBar 展示，由 FilterModal 更新）
  const [currentSearch, setCurrentSearch] = useState({
    withDate: false,
    duration: 2,
    style: 'standard',
    preference: '',
    startDate: '',
    endDate: '',
  });

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

  const handleItemClick = (item: SearchResultItem) => {
    if (item.source === 'guide' && item.daily_plan && item.daily_plan.length > 0) {
      const adapted = { ...item, daily_plan: adaptGuideDailyPlan(item.daily_plan as any) };
      setSelectedItem(adapted);
    } else {
      setSelectedItem(item);
    }
  };

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

  useEffect(() => {
    if ((activeTab === 'home' || activeTab === 'search') && seedCities.length === 0) {
      fetch('/api/cities')
        .then((r) => r.json())
        .then((d) => setSeedCities(d.cities || []))
        .catch(() => {});
    }
  }, [activeTab, seedCities.length]);

  const showToast = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(''), 2400);
  };

  const doSearch = async (input: { startDate?: string; endDate?: string; duration?: number; style?: string; sortBy?: string; preference?: string }, origin?: { province: string; city: string; county: string }) => {
    setSearchLoading(true);
    setSearchError('');
    if (input.startDate) setLastSearchParams({ startDate: input.startDate, endDate: input.endDate || '', preference: input.preference || '' });

    try {
      const results = await searchTravelPlans({
        startDate: input.startDate,
        endDate: input.endDate,
      duration: input.duration ?? 2,
        style: input.style as any,
        sortBy: input.sortBy,
        preference: input.preference,
        origin,
      });
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
      doSearch(lastSearchParams, picked);
      showToast(`出发地已更新为 ${picked.county}`);
    }
  };

  const handleSearch = async (input: { startDate?: string; endDate?: string; duration?: number; style?: string; sortBy?: string; preference: string }, origin: { province: string; city: string; county: string }) => {
    await doSearch(input, origin);
    setActiveTab('search');
  };

  const handleFilterApply = async (input: { startDate?: string; endDate?: string; duration?: number; style?: string; sortBy?: string; preference: string }) => {
    const withDate = !!(input.startDate && input.endDate);
    setCurrentSearch({
      withDate,
      startDate: input.startDate || '',
      endDate: input.endDate || '',
      duration: input.duration ?? 2,
      style: input.style ?? 'standard',
      preference: input.preference || '',
    });
    await doSearch(input, origin);
    setActiveTab('search');
    setShowFilter(false);
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
        <HomePage onSearch={(input) => handleFilterApply(input)} seedCities={seedCities} />
      )}

      {activeTab === 'search' && (
        <>
          <div className="search-bar-wrapper">
            <SearchBar
              onSearch={handleSearch}
              onOpenFilter={() => setShowFilter(true)}
              onOpenPicker={() => setShowLocationPicker(true)}
              origin={origin}
              loading={searchLoading}
              currentSearch={currentSearch}
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
                onItemClick={handleItemClick}
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
          initialValues={currentSearch}
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
