import { useState } from 'react';
import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom';
import { Icon } from './components/ui/Icon';
import { FileManagerPage } from './components/FileManagerPage';
import { SearchPage } from './components/SearchPage';
import { IndexPage } from './components/IndexPage';
import { SimpleChat } from './components/SimpleChat';
import { OrganizerPage } from './components/OrganizerPage';
import { DigestPage } from './components/DigestPage';
import { SettingsPage } from './components/SettingsPage';

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

  return (
    <BrowserRouter>
      <div className="flex h-screen overflow-hidden bg-slate-50">
        {/* Mobile overlay */}
        {mobileNavOpen && (
          <div
            className="fixed inset-0 z-30 bg-black/30 lg:hidden"
            onClick={() => setMobileNavOpen(false)}
          />
        )}

        {/* App Sidebar */}
        <aside
          className={`fixed lg:static z-40 h-full w-56 bg-white border-r border-slate-200 flex flex-col transition-transform duration-200 ${
            mobileNavOpen ? 'translate-x-0' : '-translate-x-full lg:translate-x-0'
          }`}
        >
          {/* Logo */}
          <div className="px-5 py-4 border-b border-slate-100 flex items-center gap-2.5">
            <div className="w-8 h-8 rounded-lg bg-indigo-600 flex items-center justify-center text-white">
              <Icon name="folder" size={18} />
            </div>
            <div className="text-sm font-semibold text-slate-800 tracking-tight">SimpleFileManager</div>
          </div>

          {/* Nav */}
          <nav className="flex-1 overflow-y-auto px-3 py-3 space-y-0.5">
            {NAV_ITEMS.map(item => (
              <NavLink
                key={item.to}
                to={item.to}
                onClick={() => setMobileNavOpen(false)}
                className={({ isActive }) =>
                  `flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                    isActive
                      ? 'bg-indigo-50 text-indigo-700'
                      : 'text-slate-600 hover:bg-slate-100 hover:text-slate-900'
                  }`
                }
              >
                <Icon name={item.icon} size={18} />
                {item.label}
              </NavLink>
            ))}
          </nav>

          {/* Footer */}
          <div className="px-4 py-3 border-t border-slate-100">
            <div className="text-xs text-slate-400">v0.4.0 · Local AI</div>
          </div>
        </aside>

        {/* Main */}
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Mobile top bar */}
          <div className="lg:hidden flex items-center gap-3 px-4 py-3 bg-white border-b border-slate-200 flex-shrink-0">
            <button
              className="p-2 rounded-lg text-slate-500 hover:bg-slate-100"
              onClick={() => setMobileNavOpen(true)}
            >
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
