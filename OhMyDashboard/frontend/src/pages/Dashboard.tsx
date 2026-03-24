import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { SystemMonitor } from '../components/SystemMonitor'
import { api } from '../api'
import { Box, Layers, Power, ArrowRight, Globe } from 'lucide-react'

export const Dashboard = () => {
  const [dockerCount, setDockerCount] = useState({ running: 0, total: 0 })
  const [startupCount, setStartupCount] = useState({ enabled: 0, total: 0 })
  const [netStats, setNetStats] = useState({ tcp: 0, udp: 0, total: 0 })

  useEffect(() => {
    api.getDockerInfo().then(data => {
      if (Array.isArray(data)) {
        setDockerCount({
          running: data.filter(c => c.status === 'running').length,
          total: data.length,
        })
      }
    })
    api.getStartupInfo().then(data => {
      const items = data.startup_items || []
      setStartupCount({
        enabled: items.filter(i => i.status === 'enabled').length,
        total: items.length,
      })
    })
    api.getNetworkInfo().then(data => {
      setNetStats(data.connections)
    })
  }, [])

  return (
    <div className="space-y-8">
      <section>
        <SystemMonitor />
      </section>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-5 gap-4">
        <Link to="/docker" className="flex items-center gap-4 p-5 bg-white rounded-xl border border-slate-100 hover:border-blue-200 hover:shadow-sm transition-all">
          <div className="w-12 h-12 rounded-xl bg-blue-50 flex items-center justify-center text-blue-500">
            <Layers size={22} />
          </div>
          <div>
            <p className="text-xs text-slate-400 mb-1">Docker 容器</p>
            <p className="text-xl font-bold text-slate-800">{dockerCount.running}<span className="text-sm font-normal text-slate-400">/{dockerCount.total}</span></p>
            <p className="text-xs text-slate-400">运行中 / 总数</p>
          </div>
          <ArrowRight size={16} className="ml-auto text-slate-300" />
        </Link>

        <Link to="/startup" className="flex items-center gap-4 p-5 bg-white rounded-xl border border-slate-100 hover:border-emerald-200 hover:shadow-sm transition-all">
          <div className="w-12 h-12 rounded-xl bg-emerald-50 flex items-center justify-center text-emerald-500">
            <Power size={22} />
          </div>
          <div>
            <p className="text-xs text-slate-400 mb-1">自启动服务</p>
            <p className="text-xl font-bold text-slate-800">{startupCount.enabled}<span className="text-sm font-normal text-slate-400">/{startupCount.total}</span></p>
            <p className="text-xs text-slate-400">已启用 / 总数</p>
          </div>
          <ArrowRight size={16} className="ml-auto text-slate-300" />
        </Link>

        <Link to="/process" className="flex items-center gap-4 p-5 bg-white rounded-xl border border-slate-100 hover:border-orange-200 hover:shadow-sm transition-all">
          <div className="w-12 h-12 rounded-xl bg-orange-50 flex items-center justify-center text-orange-500">
            <Box size={22} />
          </div>
          <div>
            <p className="text-xs text-slate-400 mb-1">进程管理</p>
            <p className="text-lg font-bold text-slate-800">管理</p>
            <p className="text-xs text-slate-400">查看 / 结束进程</p>
          </div>
          <ArrowRight size={16} className="ml-auto text-slate-300" />
        </Link>

        <Link to="/network" className="flex items-center gap-4 p-5 bg-white rounded-xl border border-slate-100 hover:border-cyan-200 hover:shadow-sm transition-all">
          <div className="w-12 h-12 rounded-xl bg-cyan-50 flex items-center justify-center text-cyan-500">
            <Globe size={22} />
          </div>
          <div>
            <p className="text-xs text-slate-400 mb-1">网络连接</p>
            <p className="text-xl font-bold text-slate-800">{netStats.tcp}<span className="text-sm font-normal text-slate-400">/{netStats.udp}</span></p>
            <p className="text-xs text-slate-400">TCP / UDP</p>
          </div>
          <ArrowRight size={16} className="ml-auto text-slate-300" />
        </Link>
      </div>
    </div>
  )
}
