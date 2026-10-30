import { useState, useEffect, lazy, Suspense } from 'react';
import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom';
import { Icon } from './components/ui/Icon';
import { FileManagerPage } from './components/FileManagerPage';
import { ErrorBoundary } from './components/ui/ErrorBoundary';

// The pages are split apart so that opening one does not mean
// downloading all of them. The file browser is the one that is always
// needed, so it is the one that is not lazy.
const SearchPage = lazy(() => import('./components/SearchPage').then(m => ({ default: m.SearchPage })));
const IndexPage = lazy(() => import('./components/IndexPage').then(m => ({ default: m.IndexPage })));
const SimpleChat = lazy(() => import('./components/SimpleChat').then(m => ({ default: m.SimpleChat })));
const AgentToolsPage = lazy(() => import('./components/AgentToolsPage').then(m => ({ default: m.AgentToolsPage })));
const OrganizerPage = lazy(() => import('./components/OrganizerPage').then(m => ({ default: m.OrganizerPage })));
const DigestPage = lazy(() => import('./components/DigestPage').then(m => ({ default: m.DigestPage })));
const TrashPage = lazy(() => import('./components/TrashPage').then(m => ({ default: m.TrashPage })));
const DuplicatesPage = lazy(() => import('./components/DuplicatesPage').then(m => ({ default: m.DuplicatesPage })));
const SettingsPage = lazy(() => import('./components/SettingsPage').then(m => ({ default: m.SettingsPage })));
const SharesPage = lazy(() => import('./components/SharesPage').then(m => ({ default: m.SharesPage })));
const PublicSharePage = lazy(() => import('./components/PublicSharePage').then(m => ({ default: m.PublicSharePage })));
import { LoginPage } from './components/LoginPage';
import { ServiceStatus } from './components/ServiceStatus';
import { useCollapse } from './hooks/useCollapse';
import { checkAuthStatus, getAuthEnabled, logout, onAuthLogout, verifyToken } from './auth';
import { fetchCurrentUser, fetchPlans, type CurrentUser } from './api';

function PageLoading() {
  return (
    <div className="h-full flex items-center justify-center bg-slate-50">
      <div className="flex items-center gap-3">
        <div className="loading-spinner" />
        <span className="text-slate-500 text-sm">加载中...</span>
      </div>
    </div>
  );
}

// The pages that are the administrator's business: they look at the
// whole machine. The index page is not one of them -- it shows what
// has been indexed, which a person is entitled to know about their
// own files -- but rebuilding it from there is the administrator's.
const ADMIN_ONLY = new Set(['/organizer', '/digest']);

const NAV_ITEMS = [
  { to: '/', label: '文件管理', icon: 'folder' as const },
  { to: '/search', label: '搜索', icon: 'search' as const },
  { to: '/index', label: '索引管理', icon: 'database' as const },
  { to: '/chat', label: 'Agent 对话', icon: 'chat' as const },
  { to: '/agent-tools', label: 'Agent 能力', icon: 'code' as const },
  { to: '/organizer', label: '整理', icon: 'clipboard' as const },
  { to: '/digest', label: '日报', icon: 'newspaper' as const },
  { to: '/trash', label: '回收站', icon: 'trash' as const },
  { to: '/duplicates', label: '重复文件', icon: 'copy' as const },
  { to: '/shares', label: '共享链接', icon: 'share' as const },
  { to: '/settings', label: '设置', icon: 'settings' as const },
];

function App() {
  const [mobileNavOpen, setMobileNavOpen] = useState(false);
  const { collapsed: navCollapsed, toggle: toggleNav } = useCollapse('sfm.navCollapsed');
  const [authed, setAuthed] = useState(false);
  const [authChecking, setAuthChecking] = useState(true);
  const [pendingPlanCount, setPendingPlanCount] = useState(0);
  // Who is signed in. The sidebar says so, so on a shared machine it
  // is obvious whose files are on the screen.
  const [me, setMe] = useState<CurrentUser | null>(null);

  const loadPendingPlans = () => {
    fetchPlans('pending')
      .then(plans => setPendingPlanCount(plans.length))
      .catch(() => {});
  };

  useEffect(() => { loadPendingPlans(); }, []);
  useEffect(() => { fetchCurrentUser().then(setMe).catch(() => {}); }, []);
  useEffect(() => {
    const id = setInterval(loadPendingPlans, 15000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    checkAuthStatus()
      .then(async status => {
        if (!status.requires_auth) {
          setAuthed(true);
          return;
        }
        // Token in localStorage may be expired or stale (e.g. password
        // changed on the server while we held a previous token). Verifying
        // it via the API instead of trusting localStorage breaks the
        // ghost-token loop: old token → API returns 401 → page reloads →
        // old token still there → repeat forever. verifyToken hits the
        // server's real session table and rejects bad tokens outright.
        const v = await verifyToken();
        if (v.valid) setAuthed(true);
      })
      .catch(() => setAuthed(true))
      .finally(() => setAuthChecking(false));
  }, []);

  // Listen for 401s from any authFetch call. Drops us into LoginPage
  // in place instead of doing a hard window.location.reload() —
  // that reloaded the entire React app on every API failure, and
  // multiple concurrent 401s would queue up multiple reloads.
  useEffect(() => onAuthLogout(() => setAuthed(false)), []);

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

  // A shared link is meant to be opened by somebody with no login, so
  // that route sits outside the check. It shows one thing and nothing
  // else -- a visitor should not be able to tell what else this NAS
  // holds.
  const isPublicShare = window.location.pathname.startsWith('/s/');

  if (!authed && !isPublicShare) {
    return <LoginPage onLogin={() => setAuthed(true)} />;
  }

  if (isPublicShare) {
    return (
      <BrowserRouter>
        <Suspense fallback={<PageLoading />}>
          <Routes>
            <Route path="/s/:id" element={<PublicSharePage />} />
          </Routes>
        </Suspense>
      </BrowserRouter>
    );
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
          className={`fixed lg:static z-40 h-full bg-white border-r border-slate-200 flex flex-col transition-all duration-200 ${
            navCollapsed ? 'w-14' : 'w-56'
          } ${
            mobileNavOpen ? 'translate-x-0' : '-translate-x-full lg:translate-x-0'
          }`}
        >
          {navCollapsed ? (
            /* Collapsed: the logo alone is the expander. A chevron on the
               boundary used to cross into the secondary sidebar, and stacking
               one under the logo only added clutter to a 56px rail. */
            <button
              onClick={toggleNav}
              title="展开导航"
              aria-label="展开导航"
              className="h-16 w-full flex items-center justify-center border-b border-slate-100 hover:bg-slate-50 transition-colors"
            >
              <div className="w-8 h-8 rounded-lg bg-indigo-600 flex items-center justify-center text-white">
                <Icon name="folder" size={18} />
              </div>
            </button>
          ) : (
            <div className="border-b border-slate-100 flex items-center gap-2.5 px-3 h-16">
              <div className="w-8 h-8 rounded-lg bg-indigo-600 flex items-center justify-center text-white flex-shrink-0">
                <Icon name="folder" size={18} />
              </div>
              <div className="text-sm font-semibold text-slate-800 tracking-tight truncate">SimpleFileManager</div>
              <button
                onClick={toggleNav}
                title="收起导航"
                aria-label="收起导航"
                className="ml-auto flex-shrink-0 p-2.5 sm:p-1.5 rounded-lg text-slate-400 hover:bg-slate-100 hover:text-indigo-600 transition-colors"
              >
                <Icon name="chevronRight" size={16} className="rotate-180" />
              </button>
            </div>
          )}

          <nav className={`flex-1 overflow-y-auto py-3 ${navCollapsed ? 'px-2' : 'px-3'} space-y-0.5`}>
            {NAV_ITEMS.filter(item =>
              // An ordinary person's menu does not offer pages that
              // would only be refused.
              !ADMIN_ONLY.has(item.to) || me?.is_admin !== false
            ).map(item => (
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
                {item.to === '/organizer' && pendingPlanCount > 0 && (
                  <span className="ml-auto text-[10px] px-1.5 py-0.5 rounded-full bg-amber-100 text-amber-700 font-semibold">
                    {pendingPlanCount}
                  </span>
                )}
              </NavLink>
            ))}
          </nav>

          <div className={`border-t border-slate-100 ${navCollapsed ? 'px-2 py-3' : 'px-3 py-3'}`}>
            {me?.authenticated && !navCollapsed && (
              <div className="flex items-center gap-2 px-3 pb-2 pt-1">
                <div className="w-6 h-6 rounded-full bg-slate-200 text-slate-600 flex items-center justify-center text-[11px] font-semibold flex-shrink-0">
                  {me.username.slice(0, 1).toUpperCase()}
                </div>
                <div className="min-w-0">
                  <div className="text-xs font-medium text-slate-700 truncate">{me.username}</div>
                  <div className="text-[10px] text-slate-400">{me.is_admin ? '管理员' : '普通用户'}</div>
                </div>
              </div>
            )}
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
            {/* One page failing to render must not take the others with
                it, and must not leave a blank screen. */}
            <ErrorBoundary>
              <Suspense fallback={<PageLoading />}>
                <Routes>
                  <Route path="/" element={<FileManagerPage />} />
                  <Route path="/search" element={<SearchPage />} />
                  <Route path="/index" element={<IndexPage />} />
                  <Route path="/chat" element={<SimpleChat />} />
                  <Route path="/agent-tools" element={<AgentToolsPage />} />
                  <Route path="/organizer" element={<OrganizerPage />} />
                  <Route path="/digest" element={<DigestPage />} />
                  <Route path="/trash" element={<TrashPage />} />
                  <Route path="/duplicates" element={<DuplicatesPage />} />
                  <Route path="/shares" element={<SharesPage />} />
                  <Route path="/settings" element={<SettingsPage />} />
                </Routes>
              </Suspense>
            </ErrorBoundary>
          </div>
        </div>
      </div>
    </BrowserRouter>
  );
}

export default App;
