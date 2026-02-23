import { Link, useLocation } from 'react-router-dom'

const pageNames: Record<string, string> = {
  '/': '英语学习助手',
  '/vocab': '单词本',
  '/random': '随机抽取',
  '/ocr': 'OCR识别',
}

export default function Header() {
  const location = useLocation()
  const currentTitle = pageNames[location.pathname] || '英语学习助手'

  return (
    <nav className="navbar">
      <div className="navbar-left">
        <span className="navbar-icon">📚</span>
        <span className="navbar-title">{currentTitle}</span>
      </div>
      <div className="nav-links">
        <Link to="/" className={`nav-link ${location.pathname === '/' ? 'active' : ''}`}>
          首页
        </Link>
        <Link to="/vocab" className={`nav-link ${location.pathname === '/vocab' ? 'active' : ''}`}>
          单词本
        </Link>
        <Link to="/random" className={`nav-link ${location.pathname === '/random' ? 'active' : ''}`}>
          随机抽取
        </Link>
        <Link to="/ocr" className={`nav-link ${location.pathname === '/ocr' ? 'active' : ''}`}>
          OCR识别
        </Link>
      </div>
    </nav>
  )
}
