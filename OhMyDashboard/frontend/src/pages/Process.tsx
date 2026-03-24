import { useEffect, useState, useMemo } from 'react'
import { api } from '../api'
import type { ProcessInfo } from '../types'
import { Activity, RefreshCw, Search, X, Terminal, SortAsc, SortDesc } from 'lucide-react'

type SortKey = 'cpu_percent' | 'memory_percent' | 'pid' | 'name' | 'username'
type SortDir = 'asc' | 'desc'

export const Process = () => {
  const [processes, setProcesses] = useState<ProcessInfo[]>([])
  const [isLoading, setIsLoading] = useState(true)
  const [search, setSearch] = useState('')
  const [sortKey, setSortKey] = useState<SortKey>('cpu_percent')
  const [sortDir, setSortDir] = useState<SortDir>('desc')
  const [killTarget, setKillTarget] = useState<ProcessInfo | null>(null)

  const fetchProcesses = async () => {
    setIsLoading(true)
    const data = await api.getProcesses()
    setProcesses(data)
    setIsLoading(false)
  }

  useEffect(() => {
    fetchProcesses()
  }, [])

  const handleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir(d => d === 'asc' ? 'desc' : 'asc')
    } else {
      setSortKey(key)
      setSortDir('desc')
    }
  }

  const filtered = useMemo(() => {
    let list = processes.filter(p => {
      if (!search) return true
      const s = search.toLowerCase()
      return (
        p.name.toLowerCase().includes(s) ||
        String(p.pid).includes(s) ||
        (p.username || '').toLowerCase().includes(s)
      )
    })
    list.sort((a, b) => {
      const va = a[sortKey] ?? 0
      const vb = b[sortKey] ?? 0
      const cmp = typeof va === 'string' ? va.localeCompare(vb) : (va as number) - (vb as number)
      return sortDir === 'asc' ? cmp : -cmp
    })
    return list
  }, [processes, search, sortKey, sortDir])

  const handleKill = async () => {
    if (!killTarget) return
    const res = await api.killProcess(killTarget.pid)
    if (res.status === 'success') {
      fetchProcesses()
    } else {
      alert(res.message || '结束进程失败')
    }
    setKillTarget(null)
  }

  const SortIcon = ({ k }: { k: SortKey }) => {
    if (sortKey !== k) return <SortAsc size={10} className="opacity-30" />
    return sortDir === 'asc' ? <SortAsc size={10} /> : <SortDesc size={10} />
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Activity size={20} className="text-orange-500" />
          <h2 className="text-lg font-bold text-slate-800">进程管理</h2>
          <span className="text-xs text-slate-400 ml-1">{filtered.length} 个进程</span>
        </div>
        <div className="flex items-center gap-3">
          <input
            type="text"
            placeholder="搜索进程/用户..."
            value={search}
            onChange={e => setSearch(e.target.value)}
            className="px-3 py-1.5 text-sm border border-slate-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 w-48"
          />
          <button onClick={fetchProcesses} className="p-2 hover:bg-slate-100 rounded-lg text-slate-400 hover:text-slate-600" title="刷新">
            <RefreshCw size={16} />
          </button>
        </div>
      </div>

      {isLoading ? (
        <div className="py-12 text-center text-slate-400">加载中...</div>
      ) : filtered.length === 0 ? (
        <div className="py-12 text-center text-slate-400">未找到匹配的进程</div>
      ) : (
        <div className="bg-white rounded-xl shadow-sm border border-slate-100 overflow-hidden">
          <table className="min-w-full">
            <thead>
              <tr className="border-b border-slate-100 text-slate-400 text-xs uppercase tracking-wider font-semibold">
                <th className="py-3 pl-4 w-20">
                  <button onClick={() => handleSort('pid')} className="flex items-center gap-1 hover:text-slate-600">
                    PID <SortIcon k="pid" />
                  </button>
                </th>
                <th className="py-3 w-48">
                  <button onClick={() => handleSort('name')} className="flex items-center gap-1 hover:text-slate-600">
                    进程名称 <SortIcon k="name" />
                  </button>
                </th>
                <th className="py-3 w-28">
                  <button onClick={() => handleSort('username')} className="flex items-center gap-1 hover:text-slate-600">
                    用户 <SortIcon k="username" />
                  </button>
                </th>
                <th className="py-3 w-24 text-right">
                  <button onClick={() => handleSort('cpu_percent')} className="flex items-center gap-1 ml-auto hover:text-slate-600">
                    CPU% <SortIcon k="cpu_percent" />
                  </button>
                </th>
                <th className="py-3 w-24 text-right">
                  <button onClick={() => handleSort('memory_percent')} className="flex items-center gap-1 ml-auto hover:text-slate-600">
                    内存% <SortIcon k="memory_percent" />
                  </button>
                </th>
                <th className="py-3 w-16 text-center">线程</th>
                <th className="py-3 w-20">状态</th>
                <th className="py-3 w-16 text-center">
                  <button onClick={() => setKillTarget(null)} className="hover:text-red-500" title="结束进程">
                    <X size={14} />
                  </button>
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-50">
              {filtered.map(p => (
                <tr key={p.pid} className="hover:bg-slate-50/50 transition-colors">
                  <td className="py-2.5 pl-4">
                    <span className="text-sm font-mono text-slate-500">{p.pid}</span>
                  </td>
                  <td className="py-2.5">
                    <div className="flex items-center gap-2">
                      <div className="w-6 h-6 rounded bg-orange-50 flex items-center justify-center text-orange-400">
                        <Terminal size={10} />
                      </div>
                      <span className="text-sm font-medium text-slate-700 truncate max-w-[10rem]" title={p.name}>{p.name}</span>
                    </div>
                  </td>
                  <td className="py-2.5">
                    <span className="text-xs text-slate-400">{p.username || '-'}</span>
                  </td>
                  <td className="py-2.5 text-right">
                    <div className="flex items-center gap-2 justify-end">
                      <div className="w-12 bg-slate-100 rounded-full h-1">
                        <div
                          className="h-1 rounded-full bg-orange-400"
                          style={{ width: `${Math.min(p.cpu_percent, 100)}%` }}
                        />
                      </div>
                      <span className="text-xs font-mono text-slate-500 w-10 text-right">{p.cpu_percent.toFixed(1)}</span>
                    </div>
                  </td>
                  <td className="py-2.5 text-right">
                    <div className="flex items-center gap-2 justify-end">
                      <div className="w-12 bg-slate-100 rounded-full h-1">
                        <div
                          className="h-1 rounded-full bg-blue-400"
                          style={{ width: `${Math.min(p.memory_percent, 100)}%` }}
                        />
                      </div>
                      <span className="text-xs font-mono text-slate-500 w-10 text-right">{p.memory_percent.toFixed(1)}</span>
                    </div>
                  </td>
                  <td className="py-2.5 text-center">
                    <span className="text-xs text-slate-400">{p.num_threads ?? '-'}</span>
                  </td>
                  <td className="py-2.5">
                    <span className={`text-xs px-1.5 py-0.5 rounded font-medium ${
                      p.status === 'running' ? 'bg-emerald-50 text-emerald-600' : 'bg-slate-100 text-slate-400'
                    }`}>
                      {p.status || '-'}
                    </span>
                  </td>
                  <td className="py-2.5 text-center">
                    <button
                      onClick={() => setKillTarget(p)}
                      className="p-1 hover:bg-red-50 text-slate-300 hover:text-red-500 rounded transition-colors"
                      title="结束进程"
                    >
                      <X size={12} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {killTarget && (
        <div className="fixed inset-0 bg-black/30 flex items-center justify-center z-50" onClick={() => setKillTarget(null)}>
          <div className="bg-white rounded-xl shadow-xl max-w-sm w-full mx-4 overflow-hidden" onClick={e => e.stopPropagation()}>
            <div className="px-6 py-4 border-b border-slate-100">
              <h3 className="font-bold text-slate-800">确认结束进程</h3>
            </div>
            <div className="p-6">
              <p className="text-sm text-slate-600 mb-1">
                确定要结束进程 <span className="font-mono font-bold">{killTarget.name}</span> 吗？
              </p>
              <p className="text-xs text-slate-400">
                PID: {killTarget.pid} · 用户: {killTarget.username || '-'}
              </p>
            </div>
            <div className="px-6 py-4 bg-slate-50 flex justify-end gap-3">
              <button
                onClick={() => setKillTarget(null)}
                className="px-4 py-2 text-sm text-slate-600 hover:bg-slate-100 rounded-lg transition-colors"
              >
                取消
              </button>
              <button
                onClick={handleKill}
                className="px-4 py-2 text-sm text-white bg-red-500 hover:bg-red-600 rounded-lg transition-colors"
              >
                确认结束
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
