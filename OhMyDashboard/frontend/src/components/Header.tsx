import { Link, useLocation } from 'react-router-dom'
import heroImg from '../assets/hero.png'
import { LayoutDashboard, Box, ListChecks, Activity, Globe } from 'lucide-react'

const navItems = [
  { path: '/', label: '概览', icon: LayoutDashboard },
  { path: '/docker', label: 'Docker', icon: Box },
  { path: '/startup', label: '自启项', icon: ListChecks },
  { path: '/process', label: '进程', icon: Activity },
  { path: '/network', label: '网络', icon: Globe },
]

export const Header = () => {
  const location = useLocation()

  return (
    <header className="mb-8">
      <div className="flex items-center justify-between py-4 border-b border-slate-100">
        <div className="flex items-center gap-4">
          <div className="w-12 h-12">
            <img src={heroImg} className="w-full h-full object-contain" alt="Hero" />
          </div>
          <div>
            <h1 className="text-2xl font-black bg-gradient-to-r from-blue-600 to-indigo-600 bg-clip-text text-transparent">
              Oh My Dashboard
            </h1>
            <p className="text-slate-500 text-sm">智能全栈监控与管理面板</p>
          </div>
        </div>
        <nav className="flex gap-1">
          {navItems.map(({ path, label, icon: Icon }) => (
            <Link
              key={path}
              to={path}
              className={`flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                location.pathname === path
                  ? 'bg-blue-50 text-blue-600'
                  : 'text-slate-500 hover:bg-slate-50 hover:text-slate-700'
              }`}
            >
              <Icon size={16} />
              {label}
            </Link>
          ))}
        </nav>
      </div>
    </header>
  )
}
