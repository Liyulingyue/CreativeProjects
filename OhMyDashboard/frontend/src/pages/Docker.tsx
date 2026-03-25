import { useState, useEffect } from 'react'
import { api } from '../api'
import type { DockerContainer } from '../types'
import { Box, Play, Square, RotateCcw, AlertTriangle, Layers, RefreshCw, LayoutGrid, List, Search, Terminal, Lock, X } from 'lucide-react'

type ViewMode = 'card' | 'list'
type FilterMode = 'all' | 'running' | 'stopped'

export const Docker = () => {
  const [containers, setContainers] = useState<DockerContainer[]>([])
  const [isLoading, setIsLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [needAuth, setNeedAuth] = useState(false)
  const [authError, setAuthError] = useState<string | null>(null)
  const [password, setPassword] = useState('')
  const [authLoading, setAuthLoading] = useState(false)
  const [filter, setFilter] = useState<FilterMode>('all')
  const [viewMode, setViewMode] = useState<ViewMode>('list')
  const [search, setSearch] = useState('')

  const fetchDocker = async () => {
    setIsLoading(true)
    setError(null)
    const data = await api.getDockerInfo()
    if (Array.isArray(data)) {
      setContainers(data)
    } else if (data && typeof data === 'object' && 'error' in data) {
      const d = data as { error: string; need_auth?: boolean }
      if (d.need_auth) {
        setNeedAuth(true)
        setIsLoading(false)
        return
      }
      setError(d.error)
    }
    setIsLoading(false)
  }

  const handleAuth = async () => {
    if (!password) return
    setAuthLoading(true)
    setAuthError(null)
    const res = await api.dockerAuth(password)
    if (res.status === 'ok') {
      setNeedAuth(false)
      setPassword('')
      fetchDocker()
    } else {
      setAuthError(res.message || '认证失败')
    }
    setAuthLoading(false)
  }

  useEffect(() => {
    fetchDocker()
  }, [])

  const handleAction = async (id: string, action: 'start' | 'stop' | 'restart') => {
    const res = await api.manageDocker(id, action)
    if (res.need_auth) {
      setNeedAuth(true)
      return
    }
    fetchDocker()
  }

  const filtered = containers.filter(c => {
    const keyword = search.toLowerCase()
    if (keyword && !c.name.toLowerCase().includes(keyword) && !c.image.toLowerCase().includes(keyword)) return false
    if (filter === 'running' && c.status !== 'running') return false
    if (filter === 'stopped' && c.status === 'running') return false
    return true
  })

  const getStatusBadge = (status: string) => (
    <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs font-bold uppercase ${
      status === 'running' ? 'bg-emerald-50 text-emerald-600' : 'bg-red-50 text-red-500'
    }`}>
      <span className={`w-1.5 h-1.5 rounded-full ${status === 'running' ? 'bg-emerald-500' : 'bg-red-500'}`}></span>
      {status === 'running' ? '运行中' : '已停止'}
    </span>
  )

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Layers size={20} className="text-blue-600" />
          <h2 className="text-lg font-bold text-slate-800">Docker 容器管理</h2>
          <span className="text-xs text-slate-400 ml-1">
            {containers.filter(c => c.status === 'running').length}/{containers.length} 运行中
          </span>
        </div>
        <div className="flex items-center gap-3">
          <div className="relative">
            <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-400" />
            <input
              type="text"
              placeholder="搜索容器..."
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="pl-8 pr-3 py-1.5 text-sm border border-slate-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent w-40"
            />
          </div>
          <div className="flex gap-1 bg-slate-100 rounded-lg p-1">
            {(['all', 'running', 'stopped'] as const).map(f => (
              <button
                key={f}
                onClick={() => setFilter(f)}
                className={`px-2 py-1 text-xs font-medium rounded-md transition-colors ${
                  filter === f ? 'bg-white text-slate-800 shadow-sm' : 'text-slate-500 hover:text-slate-700'
                }`}
              >
                {f === 'all' ? '全部' : f === 'running' ? '运行中' : '已停止'}
              </button>
            ))}
          </div>
          <div className="flex gap-1 bg-slate-100 rounded-lg p-1">
            <button
              onClick={() => setViewMode('list')}
              className={`p-1.5 rounded-md transition-colors ${viewMode === 'list' ? 'bg-white shadow-sm text-slate-800' : 'text-slate-400 hover:text-slate-600'}`}
              title="列表视图"
            >
              <List size={14} />
            </button>
            <button
              onClick={() => setViewMode('card')}
              className={`p-1.5 rounded-md transition-colors ${viewMode === 'card' ? 'bg-white shadow-sm text-slate-800' : 'text-slate-400 hover:text-slate-600'}`}
              title="卡片视图"
            >
              <LayoutGrid size={14} />
            </button>
          </div>
          <button
            onClick={fetchDocker}
            className="p-2 hover:bg-slate-100 rounded-lg transition-colors"
            title="刷新"
          >
            <RefreshCw size={16} className="text-slate-500" />
          </button>
        </div>
      </div>

      {isLoading ? (
        <div className="py-12 text-center text-slate-400">加载中...</div>
      ) : needAuth ? (
        <div className="py-12 flex flex-col items-center gap-4">
          <div className="w-16 h-16 rounded-full bg-red-50 flex items-center justify-center">
            <Lock size={32} className="text-red-400" />
          </div>
          <div className="text-center">
            <h3 className="font-bold text-slate-800 mb-1">需要认证</h3>
            <p className="text-sm text-slate-400">Docker 需要 sudo 权限，请输入当前用户密码</p>
          </div>
          <div className="w-full max-w-xs space-y-3">
            <input
              type="password"
              value={password}
              onChange={e => setPassword(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleAuth()}
              placeholder="输入 sudo 密码"
              className="w-full px-4 py-2.5 text-sm border border-slate-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
              autoFocus
            />
            {authError && <p className="text-xs text-red-500 text-center">{authError}</p>}
            <div className="flex gap-2">
              <button onClick={() => setNeedAuth(false)} className="flex-1 py-2 text-sm text-slate-600 bg-slate-100 hover:bg-slate-200 rounded-lg transition-colors">取消</button>
              <button onClick={handleAuth} disabled={authLoading || !password} className="flex-1 py-2 text-sm text-white bg-blue-500 hover:bg-blue-600 disabled:opacity-50 rounded-lg transition-colors">
                {authLoading ? '认证中...' : '确认'}
              </button>
            </div>
          </div>
        </div>
      ) : error ? (
        <div className="py-12 text-center text-red-400 flex flex-col items-center gap-2">
          <AlertTriangle size={24} />
          <span>{error}</span>
          <button onClick={fetchDocker} className="text-sm text-blue-500 hover:underline">重试</button>
        </div>
      ) : filtered.length === 0 ? (
        <div className="py-12 text-center text-slate-400 flex flex-col items-center gap-2">
          <AlertTriangle size={24} className="text-slate-200" />
          <span>未找到匹配的容器</span>
        </div>
      ) : viewMode === 'list' ? (
        <div className="bg-white rounded-xl shadow-sm border border-slate-100 overflow-hidden">
          <table className="min-w-full">
            <thead>
              <tr className="border-b border-slate-50 text-slate-400 text-xs uppercase tracking-wider font-semibold">
                <th className="pb-3 text-left pl-4">容器</th>
                <th className="pb-3 text-left">镜像</th>
                <th className="pb-3 text-left">状态</th>
                <th className="pb-3 text-left">端口</th>
                <th className="pb-3 text-right pr-4">操作</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-50">
              {filtered.map(c => (
                <tr key={c.id} className="hover:bg-slate-50/50 transition-colors">
                  <td className="py-3 pl-4">
                    <div className="flex items-center gap-2">
                      <div className="w-8 h-8 rounded-lg bg-blue-50 flex items-center justify-center text-blue-500">
                        <Box size={14} />
                      </div>
                      <div>
                        <span className="text-sm font-semibold text-slate-700 block">{c.name}</span>
                        <span className="text-xs text-slate-400 font-mono">{c.id}</span>
                      </div>
                    </div>
                  </td>
                  <td className="py-3">
                    <span className="text-sm text-slate-500 font-mono text-xs">{c.image}</span>
                  </td>
                  <td className="py-3">{getStatusBadge(c.status)}</td>
                  <td className="py-3">
                    <span className="text-xs text-slate-400 font-mono">
                      {c.ports && Object.keys(c.ports).length > 0 ? Object.keys(c.ports).join(', ') : '-'}
                    </span>
                  </td>
                  <td className="py-3 pr-4">
                    <div className="flex justify-end gap-1.5">
                      {c.status === 'running' ? (
                        <button onClick={() => handleAction(c.id, 'stop')} className="p-1.5 hover:bg-red-100 text-red-400 rounded-lg transition-colors" title="停止">
                          <Square size={14} fill="currentColor" />
                        </button>
                      ) : (
                        <button onClick={() => handleAction(c.id, 'start')} className="p-1.5 hover:bg-green-100 text-green-400 rounded-lg transition-colors" title="启动">
                          <Play size={14} fill="currentColor" />
                        </button>
                      )}
                      <button onClick={() => handleAction(c.id, 'restart')} className="p-1.5 hover:bg-blue-100 text-blue-400 rounded-lg transition-colors" title="重启">
                        <RotateCcw size={14} />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="px-4 py-3 border-t border-slate-50 text-xs text-slate-400 text-center">
            共 {filtered.length} 个容器
          </div>
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
          {filtered.map(c => (
            <div key={c.id} className="flex flex-col p-4 rounded-xl border border-slate-100 hover:bg-slate-50 transition-all bg-white">
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-2">
                  <div className="w-10 h-10 rounded-lg bg-blue-50 flex items-center justify-center text-blue-500">
                    <Box size={18} />
                  </div>
                  <div>
                    <span className="text-sm font-bold text-slate-700 block">{c.name}</span>
                    <span className="text-xs text-slate-400 font-mono">{c.id}</span>
                  </div>
                </div>
                {getStatusBadge(c.status)}
              </div>
              <p className="text-xs text-slate-500 mb-3 font-mono truncate" title={c.image}>
                {c.image}
              </p>
              <div className="mt-auto pt-3 border-t border-slate-50">
                <div className="flex items-center justify-between text-xs text-slate-400 mb-3">
                  <span>端口: {c.ports && Object.keys(c.ports).length > 0 ? Object.keys(c.ports).slice(0, 2).join(', ') + (Object.keys(c.ports).length > 2 ? '...' : '') : '-'}</span>
                </div>
                <div className="flex gap-1.5">
                  {c.status === 'running' ? (
                    <button onClick={() => handleAction(c.id, 'stop')} className="flex-1 py-1.5 hover:bg-red-50 text-red-400 rounded-lg transition-colors text-xs font-medium" title="停止">
                      停止
                    </button>
                  ) : (
                    <button onClick={() => handleAction(c.id, 'start')} className="flex-1 py-1.5 hover:bg-green-50 text-green-400 rounded-lg transition-colors text-xs font-medium" title="启动">
                      启动
                    </button>
                  )}
                  <button onClick={() => handleAction(c.id, 'restart')} className="flex-1 py-1.5 hover:bg-blue-50 text-blue-400 rounded-lg transition-colors text-xs font-medium" title="重启">
                    重启
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
