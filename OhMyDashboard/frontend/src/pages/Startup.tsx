import { useEffect, useState } from 'react'
import { api } from '../api'
import type { StartupItem } from '../types'
import { Power, RefreshCw, ListChecks, ToggleLeft, ToggleRight, LayoutGrid, List, Search, X, Info } from 'lucide-react'

type ViewMode = 'card' | 'list'
type FilterMode = 'all' | 'enabled' | 'disabled'

export const Startup = () => {
  const [items, setItems] = useState<StartupItem[]>([])
  const [isLoading, setIsLoading] = useState(true)
  const [viewMode, setViewMode] = useState<ViewMode>('list')
  const [filterMode, setFilterMode] = useState<FilterMode>('all')
  const [search, setSearch] = useState('')
  const [selectedItem, setSelectedItem] = useState<StartupItem | null>(null)

  const fetchStartup = async () => {
    setIsLoading(true)
    const data = await api.getStartupInfo()
    setItems(data.startup_items || [])
    setIsLoading(false)
  }

  useEffect(() => {
    fetchStartup()
  }, [])

  const filtered = items.filter(item => {
    const name = item.name.toLowerCase()
    const desc = (item.description || '').toLowerCase()
    const keyword = search.toLowerCase()
    if (keyword && !name.includes(keyword) && !desc.includes(keyword)) return false
    if (filterMode === 'enabled' && item.status !== 'enabled') return false
    if (filterMode === 'disabled' && item.status === 'disabled') return false
    return true
  })

  const enabledCount = items.filter(i => i.status === 'enabled').length

  const getStatusBadge = (item: StartupItem) => {
    const isEnabled = item.status === 'enabled'
    return (
      <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs font-bold uppercase ${
        isEnabled ? 'bg-emerald-50 text-emerald-600' : 'bg-slate-100 text-slate-500'
      }`}>
        <span className={`w-1.5 h-1.5 rounded-full ${isEnabled ? 'bg-emerald-500' : 'bg-slate-400'}`}></span>
        {isEnabled ? '启用' : '禁用'}
      </span>
    )
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <ListChecks size={20} className="text-emerald-600" />
          <h2 className="text-lg font-bold text-slate-800">系统自启项管理</h2>
          <span className="text-xs text-slate-400 ml-1">
            {enabledCount}/{items.length} 已启用
          </span>
        </div>
        <div className="flex items-center gap-4">
          <input
            type="text"
            placeholder="搜索服务..."
            value={search}
            onChange={e => setSearch(e.target.value)}
            className="px-3 py-1.5 text-sm border border-slate-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent w-40"
          />
          <select
            value={filterMode}
            onChange={e => setFilterMode(e.target.value as FilterMode)}
            className="px-2 py-1.5 text-sm border border-slate-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 bg-white"
          >
            <option value="all">全部</option>
            <option value="enabled">已启用</option>
            <option value="disabled">已禁用</option>
          </select>
          <button
            onClick={() => setViewMode(viewMode === 'list' ? 'card' : 'list')}
            className="p-2 hover:bg-slate-100 rounded-lg transition-colors text-slate-400 hover:text-slate-600"
            title="切换视图"
          >
            {viewMode === 'list' ? <LayoutGrid size={16} /> : <List size={16} />}
          </button>
          <button onClick={fetchStartup} className="p-2 hover:bg-slate-100 rounded-lg transition-colors text-slate-400 hover:text-slate-600" title="刷新">
            <RefreshCw size={16} />
          </button>
        </div>
      </div>

      {isLoading ? (
        <div className="py-12 text-center text-slate-400">加载中...</div>
      ) : filtered.length === 0 ? (
        <div className="py-12 text-center text-slate-400">未找到匹配的服务</div>
      ) : viewMode === 'list' ? (
        <div className="bg-white rounded-xl shadow-sm border border-slate-100 overflow-hidden">
          <table className="min-w-full">
            <thead>
              <tr className="border-b border-slate-100 text-slate-400 text-xs uppercase tracking-wider font-semibold">
                <th className="py-3 pl-4">服务</th>
                <th className="py-3">描述</th>
                <th className="py-3 w-20">激活</th>
                <th className="py-3 w-20">运行</th>
                <th className="py-3 w-20">Vendor</th>
                <th className="py-3 w-16 text-right">PID</th>
                <th className="py-3 w-20 text-center">自启</th>
                <th className="py-3 w-12"></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-50">
              {filtered.map((item, idx) => (
                <tr key={idx} className="hover:bg-slate-50/50 transition-colors">
                  <td className="py-3 pl-4">
                    <div className="flex items-center gap-2">
                      <div className="w-6 h-6 rounded bg-emerald-50 flex items-center justify-center text-emerald-500">
                        <Power size={10} />
                      </div>
                      <span className="text-sm font-semibold text-slate-700 font-mono">{item.name}</span>
                    </div>
                  </td>
                  <td className="py-3">
                    <span className="text-sm text-slate-500">{item.description || '-'}</span>
                  </td>
                  <td className="py-3">
                    <span className={`text-xs font-medium ${item.active_state === 'active' ? 'text-emerald-600' : 'text-slate-400'}`}>
                      {item.active_state || '-'}
                    </span>
                  </td>
                  <td className="py-3">
                    <span className="text-xs text-slate-400">{item.sub_state || '-'}</span>
                  </td>
                  <td className="py-3">
                    <span className="text-xs text-slate-400">{item.vendor_preset || '-'}</span>
                  </td>
                  <td className="py-3 text-right">
                    <span className="text-xs font-mono text-slate-400">{item.main_pid && item.main_pid !== '-' ? item.main_pid : '-'}</span>
                  </td>
                  <td className="py-3 text-center">
                    {item.status === 'enabled' ? (
                      <ToggleRight className="inline text-emerald-500" size={18} fill="currentColor" opacity={0.6} />
                    ) : (
                      <ToggleLeft className="inline text-slate-300" size={18} />
                    )}
                  </td>
                  <td className="py-3 text-center">
                    <button
                      onClick={() => setSelectedItem(selectedItem?.name === item.name ? null : item)}
                      className="p-1 hover:bg-slate-100 rounded"
                      title="详情"
                    >
                      <Info size={12} className="text-slate-400" />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="px-4 py-2 border-t border-slate-50 text-xs text-slate-400 text-center">
            共 {filtered.length} 个服务
          </div>
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
          {filtered.map((item, idx) => (
            <div key={idx} className="flex flex-col p-4 rounded-xl border border-slate-100 hover:bg-slate-50 transition-all bg-white">
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2">
                  <div className="w-8 h-8 rounded-lg bg-emerald-50 flex items-center justify-center text-emerald-500">
                    <Power size={14} />
                  </div>
                  <span className="text-sm font-bold text-slate-700 font-mono truncate" title={item.name}>
                    {item.name.replace('.service', '')}
                  </span>
                </div>
                {getStatusBadge(item)}
              </div>
              <p className="text-xs text-slate-500 mb-3 line-clamp-2 min-h-[2rem]">
                {item.description || '无描述'}
              </p>
              <div className="flex items-center justify-between text-xs text-slate-400 mt-auto pt-3 border-t border-slate-50">
                <span>激活: <span className={item.active_state === 'active' ? 'text-emerald-500' : ''}>{item.active_state || '-'}</span></span>
                <button onClick={() => setSelectedItem(selectedItem?.name === item.name ? null : item)} className="text-blue-500 hover:text-blue-600">详情</button>
              </div>
            </div>
          ))}
        </div>
      )}

      {selectedItem && (
        <div className="fixed inset-0 bg-black/30 flex items-center justify-center z-50" onClick={() => setSelectedItem(null)}>
          <div className="bg-white rounded-xl shadow-xl max-w-lg w-full mx-4 overflow-hidden" onClick={e => e.stopPropagation()}>
            <div className="flex items-center justify-between px-6 py-4 border-b border-slate-100">
              <div className="flex items-center gap-3">
                <div className="w-10 h-10 rounded-lg bg-emerald-50 flex items-center justify-center text-emerald-500">
                  <Power size={18} />
                </div>
                <div>
                  <h3 className="font-bold text-slate-800 font-mono">{selectedItem.name}</h3>
                  <p className="text-xs text-slate-400">{selectedItem.description || '无描述'}</p>
                </div>
              </div>
              <button onClick={() => setSelectedItem(null)} className="p-2 hover:bg-slate-100 rounded-lg">
                <X size={16} className="text-slate-400" />
              </button>
            </div>
            <div className="p-6 space-y-3">
              {[
                ['自启动状态', selectedItem.status, selectedItem.status === 'enabled' ? 'text-emerald-600' : 'text-slate-400'],
                ['Vendor 预设', selectedItem.vendor_preset || '-', 'text-slate-500'],
                ['激活状态', selectedItem.active_state || '-', selectedItem.active_state === 'active' ? 'text-emerald-600' : 'text-slate-400'],
                ['运行状态', selectedItem.sub_state || '-', 'text-slate-500'],
                ['加载状态', selectedItem.load_state || '-', 'text-slate-500'],
                ['主进程 PID', selectedItem.main_pid || '-', 'text-slate-500'],
                ['内存占用', selectedItem.memory_current || '-', 'text-slate-500'],
              ].map(([label, value, cls]) => (
                <div key={label as string} className="flex items-center justify-between text-sm">
                  <span className="text-slate-400">{label as string}</span>
                  <span className={`font-medium ${cls as string}`}>{value as string}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      <div className="text-xs text-slate-400 text-center">
        仅显示 Systemd 服务 · 点击详情查看完整信息 · 切换自启状态功能待实现
      </div>
    </div>
  )
}
