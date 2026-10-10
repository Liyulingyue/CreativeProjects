import { useState, useEffect } from 'react';
import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom';
import { Icon } from './components/ui/Icon';
import { FileManagerPage } from './components/FileManagerPage';
import { SearchPage } from './components/SearchPage';
import { IndexPage } from './components/IndexPage';
import { SimpleChat } from './components/SimpleChat';
import { OrganizerPage } from './components/OrganizerPage';
import { DigestPage } from './components/DigestPage';
import { SettingsPage } from './components/SettingsPage';
import { LoginPage } from './components/LoginPage';
import { ServiceStatus } from './components/ServiceStatus';
import { useCollapse } from './hooks/useCollapse';
import { checkAuthStatus, getToken, getAuthEnabled, logout } from './auth';

const NAV_ITEMS = [
  { to: '/', label: '文件管理', icon: 'folder' as const },
  { to: '/search', label: '搜索', icon: 'search' as const },
  { to: '/index', label: '索引管理', icon: 'database' as const },
  { to: '/chat', label: 'Agent 对话', icon: 'chat' as const },
  { to: '/organizer', label: '整理', icon: 'clipboard' as const },
  { to: '/digest', label: '日报', icon: 'newspaper' as const },
  { to: '/settings', label: '设置', icon: 'settings' as const },
];

function App() {
  const [mobileNavOpen, setMobileNavOpen] = useState(false);
  const { collapsed: navCollapsed, toggle: toggleNav } = useCollapse('sfm.navCollapsed');
  const [authed, setAuthed] = useState(false);
  const [authChecking, setAuthChecking] = useState(true);

  useEffect(() => {
    checkAuthStatus()
      .then(status => {
        if (!status.requires_auth) {
          setAuthed(true);
        } else if (getToken()) {
          setAuthed(true);
        }
      })
      .catch(() => setAuthed(true))
      .finally(() => setAuthChecking(false));
  }, []);

  const handleLogout = async () => {
    await logout();
    setAuthed(false);
  };

  if (authChecking) {
    return (
      <div className="flex h-screen items-center justify-center bg-slate-50">
        <div className="loading-spinner" />
      </div>
    );
  }

  if (!authed) {
    return <LoginPage onLogin={() => setAuthed(true)} />;
  }

  return (
    <BrowserRouter>
      <div className="flex h-screen overflow-hidden bg-slate-50">
        {/* Mobile overlay */}
        {mobileNavOpen && (
          <div className="fixed inset-0 z-30 bg-black/30 lg:hidden" onClick={() => setMobileNavOpen(false)} />
        )}

        {/* App Sidebar */}
        <aside
          className={`fixed lg:relative z-40 h-full bg-white border-r border-slate-200 flex flex-col transition-all duration-200 ${
            navCollapsed ? 'w-14' : 'w-56'
          } ${
            mobileNavOpen ? 'translate-x-0' : '-translate-x-full lg:translate-x-0'
          }`}
        >
          <div className={`border-b border-slate-100 flex items-center h-16 ${navCollapsed ? 'justify-center px-2' : 'gap-2.5 px-3'}`}>
            <div className="w-8 h-8 rounded-lg bg-indigo-600 flex items-center justify-center text-white flex-shrink-0">
              <Icon name="folder" size={18} />
            </div>
            {!navCollapsed && (
              <>
                <div className="text-sm font-semibold text-slate-800 tracking-tight truncate">SimpleFileManager</div>
                <button
                  onClick={toggleNav}
                  title="收起导航"
                  aria-label="收起导航"
                  className="ml-auto flex-shrink-0 p-1.5 rounded-lg text-slate-400 hover:bg-slate-100 hover:text-indigo-600 transition-colors"
                >
                  <Icon name="chevronRight" size={16} className="rotate-180" />
                </button>
              </>
            )}
          </div>

          <nav className={`flex-1 overflow-y-auto py-3 ${navCollapsed ? 'px-2' : 'px-3'} space-y-0.5`}>
            {NAV_ITEMS.map(item => (
              <NavLink
                key={item.to}
                to={item.to}
                onClick={() => setMobileNavOpen(false)}
                title={navCollapsed ? item.label : undefined}
                className={({ isActive }) =>
                  `flex items-center rounded-lg text-sm font-medium transition-colors ${
                    navCollapsed ? 'justify-center px-2 py-2.5' : 'gap-3 px-3 py-2'
                  } ${
                    isActive ? 'bg-indigo-50 text-indigo-700' : 'text-slate-600 hover:bg-slate-100 hover:text-slate-900'
                  }`
                }
              >
                <Icon name={item.icon} size={18} />
                {!navCollapsed && item.label}
              </NavLink>
            ))}
          </nav>

          <div className={`border-t border-slate-100 ${navCollapsed ? 'px-2 py-3' : 'px-3 py-3'}`}>
            {getAuthEnabled() && (
              <button
                onClick={handleLogout}
                title={navCollapsed ? '退出登录' : undefined}
                className={`w-full flex items-center rounded-lg text-sm font-medium text-slate-600 hover:bg-slate-100 transition-colors ${
                  navCollapsed ? 'justify-center px-2 py-2' : 'gap-3 px-3 py-2'
                }`}
              >
                <Icon name="arrowLeft" size={18} />
                {!navCollapsed && '退出登录'}
              </button>
            )}
            {!navCollapsed && (
              <div className="px-3 pt-2 text-xs text-slate-400">v0.4.0 · Local AI</div>
            )}
            <div className={navCollapsed ? '' : 'mt-2'}>
              <ServiceStatus collapsed={navCollapsed} />
            </div>
          </div>

          {/* Rendered only when collapsed: a conditional `hidden` class loses to
                `lg:flex` in Tailwind's cascade, which showed this handle
                alongside the header toggle. */}
          {navCollapsed && (
            <button
              onClick={toggleNav}
              title="展开导航"
              aria-label="展开导航"
              className="absolute -right-2.5 top-3.5 hidden lg:flex items-center justify-center text-slate-400 hover:text-indigo-600 transition-colors"
            >
              <Icon name="chevronRight" size={16} />
            </button>
          )}
        </aside>

        {/* Main */}
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="lg:hidden flex items-center gap-3 px-4 py-3 bg-white border-b border-slate-200 flex-shrink-0">
            <button className="p-2 rounded-lg text-slate-500 hover:bg-slate-100" onClick={() => setMobileNavOpen(true)}>
              <Icon name="menu" size={20} />
            </button>
            <div className="flex items-center gap-2">
              <div className="w-7 h-7 rounded-lg bg-indigo-600 flex items-center justify-center text-white">
                <Icon name="folder" size={16} />
              </div>
              <span className="text-sm font-semibold text-slate-800">SimpleFileManager</span>
            </div>
          </div>

          <div className="flex-1 overflow-hidden">
            <Routes>
              <Route path="/" element={<FileManagerPage />} />
              <Route path="/search" element={<SearchPage />} />
              <Route path="/index" element={<IndexPage />} />
              <Route path="/chat" element={<SimpleChat />} />
              <Route path="/organizer" element={<OrganizerPage />} />
              <Route path="/digest" element={<DigestPage />} />
              <Route path="/settings" element={<SettingsPage />} />
            </Routes>
          </div>
        </div>
      </div>
    </BrowserRouter>
  );
}

export default App;
