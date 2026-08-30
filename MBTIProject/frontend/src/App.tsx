import { BrowserRouter, Link, Route, Routes } from 'react-router-dom'
import Home from './pages/Home'
import Take from './pages/Take'
import Report from './pages/Report'
import Generate from './pages/Generate'
import Evaluate from './pages/Evaluate'

export default function App() {
  return (
    <BrowserRouter>
      <header className="nav">
        <Link to="/" className="nav-brand">🧠 测评实验室</Link>
        <nav className="nav-links">
          <Link to="/">测评广场</Link>
          <Link to="/generate">AI 生成测评</Link>
        </nav>
      </header>
      <main className="container">
        <Routes>
          <Route path="/" element={<Home />} />
          <Route path="/take/:assessmentId" element={<Take />} />
          <Route path="/report/:sessionId" element={<Report />} />
          <Route path="/generate" element={<Generate />} />
          <Route path="/evaluate/:assessmentId" element={<Evaluate />} />
        </Routes>
      </main>
      <footer className="footer">MBTIProject · 通用测评平台原型</footer>
    </BrowserRouter>
  )
}
