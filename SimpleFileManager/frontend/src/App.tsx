import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom';
import { FileManagerPage } from './components/FileManagerPage';
import { SearchPage } from './components/SearchPage';
import { IndexPage } from './components/IndexPage';
import { SimpleChat } from './components/SimpleChat';

function App() {
  return (
    <BrowserRouter>
      <div className="app h-screen flex flex-col">
        <header className="flex items-center justify-between px-6 py-4 bg-white border-b border-slate-100">
          <h1 className="text-xl font-bold text-slate-900">📁 SimpleFileManager</h1>
          <nav className="flex gap-2">
            <NavLink
              to="/"
              className={({ isActive }) =>
                `px-4 py-2 rounded-xl text-sm font-medium transition-colors ${
                  isActive ? 'bg-indigo-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
                }`
              }
            >
              📂 文件管理
            </NavLink>
            <NavLink
              to="/search"
              className={({ isActive }) =>
                `px-4 py-2 rounded-xl text-sm font-medium transition-colors ${
                  isActive ? 'bg-indigo-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
                }`
              }
            >
              🔍 搜索
            </NavLink>
            <NavLink
              to="/index"
              className={({ isActive }) =>
                `px-4 py-2 rounded-xl text-sm font-medium transition-colors ${
                  isActive ? 'bg-indigo-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
                }`
              }
            >
              📊 索引管理
            </NavLink>
            <NavLink
              to="/chat"
              className={({ isActive }) =>
                `px-4 py-2 rounded-xl text-sm font-medium transition-colors ${
                  isActive ? 'bg-indigo-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
                }`
              }
            >
              🤖 Agent 对话
            </NavLink>
          </nav>
        </header>

        <div className="flex-1 overflow-hidden">
          <Routes>
            <Route path="/" element={<FileManagerPage />} />
            <Route path="/search" element={<SearchPage />} />
            <Route path="/index" element={<IndexPage />} />
            <Route path="/chat" element={<SimpleChat />} />
          </Routes>
        </div>
      </div>
    </BrowserRouter>
  );
}

export default App;
