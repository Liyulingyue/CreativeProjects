import { useState } from 'react';
import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom';
import { FileManagerPage } from './components/FileManagerPage';
import { SearchPage } from './components/SearchPage';
import { IndexPage } from './components/IndexPage';
import { SimpleChat } from './components/SimpleChat';
import { OrganizerPage } from './components/OrganizerPage';
import { DigestPage } from './components/DigestPage';
import { SettingsPage } from './components/SettingsPage';

const NAV_ITEMS = [
  { to: '/', label: '文件管理', icon: '📂' },
  { to: '/search', label: '搜索', icon: '🔍' },
  { to: '/index', label: '索引管理', icon: '📊' },
  { to: '/chat', label: 'Agent 对话', icon: '🤖' },
  { to: '/organizer', label: '整理', icon: '📋' },
  { to: '/digest', label: '日报', icon: '📰' },
  { to: '/settings', label: '设置', icon: '⚙️' },
];

function App() {
  const [mobileNavOpen, setMobileNavOpen] = useState(false);

  return (
    <BrowserRouter>
      <div className="app h-screen flex flex-col">
        <header className="flex items-center justify-between px-4 sm:px-6 py-3 bg-white border-b border-slate-200 flex-shrink-0">
          <div className="flex items-center gap-3">
            <button
              className="lg:hidden p-2 rounded-lg text-slate-500 hover:bg-slate-100 transition-colors"
              onClick={() => setMobileNavOpen(v => !v)}
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                {mobileNavOpen ? (
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                ) : (
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
                )}
              </svg>
            </button>
            <h1 className="text-lg font-bold text-slate-900 whitespace-nowrap">📁 SimpleFileManager</h1>
          </div>

          {/* Desktop nav */}
          <nav className="hidden lg:flex gap-2">
            {NAV_ITEMS.map(item => (
              <NavLink
                key={item.to}
                to={item.to}
                className={({ isActive }) =>
                  `px-4 py-2 rounded-xl text-sm font-medium transition-colors whitespace-nowrap ${
                    isActive ? 'bg-indigo-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
                  }`
                }
              >
                {item.icon} {item.label}
              </NavLink>
            ))}
          </nav>
        </header>

        {/* Mobile nav */}
        {mobileNavOpen && (
          <nav className="lg:hidden flex flex-wrap gap-2 px-4 py-3 bg-white border-b border-slate-200">
            {NAV_ITEMS.map(item => (
              <NavLink
                key={item.to}
                to={item.to}
                onClick={() => setMobileNavOpen(false)}
                className={({ isActive }) =>
                  `px-3 py-2 rounded-xl text-sm font-medium transition-colors ${
                    isActive ? 'bg-indigo-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
                  }`
                }
              >
                {item.icon} {item.label}
              </NavLink>
            ))}
          </nav>
        )}

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
    </BrowserRouter>
  );
}

export default App;
