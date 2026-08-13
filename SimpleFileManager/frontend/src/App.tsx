import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom';
import { FileManagerPage } from './components/FileManagerPage';
import { RAGPanel } from './components/RAGPanel';
import { SimpleChat } from './components/SimpleChat';

function App() {
  return (
    <BrowserRouter>
      <div className="app">
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
              to="/rag"
              className={({ isActive }) =>
                `px-4 py-2 rounded-xl text-sm font-medium transition-colors ${
                  isActive ? 'bg-indigo-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
                }`
              }
            >
              📚 AI 问答
            </NavLink>
            <NavLink
              to="/chat"
              className={({ isActive }) =>
                `px-4 py-2 rounded-xl text-sm font-medium transition-colors ${
                  isActive ? 'bg-indigo-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
                }`
              }
            >
              💬 单纯对话
            </NavLink>
          </nav>
        </header>

        <Routes>
          <Route path="/" element={<FileManagerPage />} />
          <Route path="/rag" element={<RAGPanel />} />
          <Route path="/chat" element={<SimpleChat />} />
        </Routes>
      </div>
    </BrowserRouter>
  );
}

export default App;
