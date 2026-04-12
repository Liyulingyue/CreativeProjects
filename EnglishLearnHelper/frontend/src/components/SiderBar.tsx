import { useState, useEffect } from 'react'
import { Link, useLocation } from 'react-router-dom'

const pageNames: Record<string, string> = {
  '/': '英语学习助手',
  '/vocab': '单词本',
  '/random': '随机抽取',
  '/settings': '设置',
}

const navItems = [
  { path: '/', label: '首页', icon: '🏠' },
  { path: '/vocab', label: '单词本', icon: '📖' },
  { path: '/random', label: '随机抽取', icon: '🎲' },
  { path: '/settings', label: '设置', icon: '⚙️' },
]

function Header() {
  const location = useLocation()
  const currentTitle = pageNames[location.pathname] || '英语学习助手'

  return (
    <nav className="header-bar">
      <div className="header-left">
        <span className="header-icon">📚</span>
        <span className="header-title">{currentTitle}</span>
      </div>
      <div className="header-links">
        {navItems.map(item => (
          <Link 
            key={item.path} 
            to={item.path} 
            className={`header-link ${location.pathname === item.path ? 'active' : ''}`}
          >
            {item.label}
          </Link>
        ))}
      </div>
    </nav>
  )
}

function MobileSider({ isOpen, onToggle }: { isOpen: boolean; onToggle: () => void }) {
  const location = useLocation()
  const currentTitle = pageNames[location.pathname] || '英语学习助手'

  return (
    <>
      <button 
        className={`sider-toggle ${isOpen ? 'open' : ''}`} 
        onClick={onToggle}
        aria-label="切换侧边栏"
      >
        <span className="sider-toggle-icon">{isOpen ? '✕' : '☰'}</span>
      </button>

      <aside className={`siderbar ${isOpen ? 'open' : ''}`}>
        <div className="siderbar-header">
          <span className="siderbar-icon">📚</span>
          <span className="siderbar-title">{currentTitle}</span>
        </div>
        <nav className="siderbar-nav">
          {navItems.map(item => (
            <Link 
              key={item.path} 
              to={item.path} 
              className={`siderbar-link ${location.pathname === item.path ? 'active' : ''}`}
              onClick={onToggle}
            >
              <span className="siderbar-link-icon">{item.icon}</span>
              <span className="siderbar-link-label">{item.label}</span>
            </Link>
          ))}
        </nav>
      </aside>

      {isOpen && <div className="siderbar-overlay" onClick={onToggle} />}
    </>
  )
}

export default function SiderBar() {
  const [isMobile, setIsMobile] = useState(false)
  const [isOpen, setIsOpen] = useState(false)

  useEffect(() => {
    const checkMobile = () => setIsMobile(window.innerWidth < 768)
    checkMobile()
    window.addEventListener('resize', checkMobile)
    return () => window.removeEventListener('resize', checkMobile)
  }, [])

  if (!isMobile) {
    return <Header />
  }

  return <MobileSider isOpen={isOpen} onToggle={() => setIsOpen(!isOpen)} />
}