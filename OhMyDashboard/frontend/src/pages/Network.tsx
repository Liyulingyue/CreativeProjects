import { useEffect, useState, useRef } from 'react'
import { api } from '../api'
import type { NetworkInfo } from '../types'
import { Wifi, Globe, RefreshCw, ArrowUp, ArrowDown, Activity, Server, Radio, Link } from 'lucide-react'

const fmt = (b: number) => {
  if (b === 0) return '0 B'
  const k = 1024
  const s = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(b) / Math.log(k))
  return `${(b / Math.pow(k, i)).toFixed(1)} ${s[i]}`
}
const fmtS = (b: number) => {
  if (b === 0) return '0 B/s'
  const k = 1024
  const s = ['B/s', 'KB/s', 'MB/s', 'GB/s']
  const i = Math.floor(Math.log(b) / Math.log(k))
  return `${(b / Math.pow(k, i)).toFixed(1)} ${s[i]}`
}

export const Network = () => {
  const [info, setInfo] = useState<NetworkInfo | null>(null)
  const [prevStats, setPrevStats] = useState<Record<string, { sent: number; recv: number; time: number }>>({})
  const [speeds, setSpeeds] = useState<Record<string, { up: number; down: number }>>({})
  const [isLoading, setIsLoading] = useState(true)
  const [selectedIface, setSelectedIface] = useState<string>('')
  const [tab, setTab] = useState<'interfaces' | 'connections' | 'traffic' | 'ports'>('interfaces')
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const historyRef = useRef<{ up: number; down: number }[]>([])

  const fetchNetwork = async () => {
    const data = await api.getNetworkInfo()
    const now = Date.now()

    const newSpeeds: Record<string, { up: number; down: number }> = {}
    for (const [iface, ifaceData] of Object.entries(data.interfaces)) {
      const prev = prevStats[iface]
      if (prev) {
        const elapsed = (now - prev.time) / 1000
        if (elapsed > 0) {
          newSpeeds[iface] = {
            down: Math.max(0, (ifaceData.bytes_recv - prev.recv) / elapsed),
            up: Math.max(0, (ifaceData.bytes_sent - prev.sent) / elapsed),
          }
          if (iface === selectedIface) {
            const next = [...historyRef.current, newSpeeds[iface]]
            historyRef.current = next.slice(-60)
            drawChart(historyRef.current)
          }
        }
      }
      setPrevStats(p => ({
        ...p,
        [iface]: { sent: ifaceData.bytes_sent, recv: ifaceData.bytes_recv, time: now }
      }))
    }
    setSpeeds(newSpeeds)
    setInfo(data)
    setIsLoading(false)
  }

  const drawChart = (history: { up: number; down: number }[]) => {
    const canvas = canvasRef.current
    if (!canvas || history.length < 2) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const w = canvas.width
    const h = canvas.height
    ctx.clearRect(0, 0, w, h)

    const maxVal = Math.max(...history.map(h => Math.max(h.up, h.down)), 1024)

    const drawLine = (data: number[], color: string) => {
      ctx.beginPath()
      ctx.strokeStyle = color
      ctx.lineWidth = 2
      ctx.lineJoin = 'round'
      ctx.lineCap = 'round'
      data.forEach((v, i) => {
        const x = (i / (history.length - 1)) * w
        const y = h - (v / maxVal) * (h - 8)
        if (i === 0) ctx.moveTo(x, y)
        else ctx.lineTo(x, y)
      })
      ctx.stroke()
    }

    drawLine(history.map(h => h.down), '#10b981')
    drawLine(history.map(h => h.up), '#3b82f6')
  }

  useEffect(() => {
    fetchNetwork()
    const timer = setInterval(fetchNetwork, 1000)
    return () => clearInterval(timer)
  }, [])

  useEffect(() => {
    if (!info || selectedIface) return
    const ifaces = Object.keys(info.interfaces)
    if (ifaces.length > 0) {
      setSelectedIface(ifaces[0])
    }
  }, [info])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    canvas.width = canvas.offsetWidth * 2
    canvas.height = 100 * 2
    const ctx = canvas.getContext('2d')
    if (ctx) {
      ctx.scale(2, 2)
    }
  }, [])

  if (isLoading || !info) {
    return <div className="py-12 text-center text-slate-400">加载中...</div>
  }

  const activeIfaces = Object.entries(info.interfaces).filter(([, v]) => v.is_up)
  const ifaceData = selectedIface ? info.interfaces[selectedIface] : null

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Globe size={20} className="text-cyan-500" />
          <h2 className="text-lg font-bold text-slate-800">网络监控</h2>
        </div>
        <div className="flex gap-3">
          <div className="flex gap-1 bg-slate-100 rounded-lg p-1">
            {(['interfaces', 'connections', 'traffic', 'ports'] as const).map(t => (
              <button
                key={t}
                onClick={() => setTab(t)}
                className={`px-3 py-1 text-xs font-medium rounded-md transition-colors ${
                  tab === t ? 'bg-white text-slate-800 shadow-sm' : 'text-slate-500 hover:text-slate-700'
                }`}
              >
                {t === 'interfaces' ? '网卡' : t === 'connections' ? '连接' : t === 'traffic' ? '流量' : '端口'}
              </button>
            ))}
          </div>
          <button onClick={fetchNetwork} className="p-2 hover:bg-slate-100 rounded-lg text-slate-400 hover:text-slate-600" title="刷新">
            <RefreshCw size={16} />
          </button>
        </div>
      </div>

      {tab === 'interfaces' && (
        <div className="space-y-4">
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
            <div className="bg-white p-4 rounded-xl border border-slate-100">
              <div className="flex items-center gap-2 mb-2">
                <ArrowDown size={14} className="text-emerald-500" />
                <span className="text-xs text-slate-400">总接收</span>
              </div>
              <p className="text-lg font-bold text-slate-800">{fmt(info.total.bytes_recv)}</p>
            </div>
            <div className="bg-white p-4 rounded-xl border border-slate-100">
              <div className="flex items-center gap-2 mb-2">
                <ArrowUp size={14} className="text-blue-500" />
                <span className="text-xs text-slate-400">总发送</span>
              </div>
              <p className="text-lg font-bold text-slate-800">{fmt(info.total.bytes_sent)}</p>
            </div>
            <div className="bg-white p-4 rounded-xl border border-slate-100">
              <div className="flex items-center gap-2 mb-2">
                <Activity size={14} className="text-orange-500" />
                <span className="text-xs text-slate-400">TCP 连接</span>
              </div>
              <p className="text-lg font-bold text-slate-800">{info.connections.tcp}</p>
            </div>
            <div className="bg-white p-4 rounded-xl border border-slate-100">
              <div className="flex items-center gap-2 mb-2">
                <Server size={14} className="text-purple-500" />
                <span className="text-xs text-slate-400">UDP 连接</span>
              </div>
              <p className="text-lg font-bold text-slate-800">{info.connections.udp}</p>
            </div>
          </div>

          <div className="bg-slate-50 rounded-lg p-1 flex gap-1 w-fit overflow-x-auto">
            {activeIfaces.map(([name]) => (
              <button
                key={name}
                onClick={() => { setSelectedIface(name); historyRef.current = [] }}
                className={`px-4 py-1.5 text-sm font-medium rounded-md transition-colors flex items-center gap-2 whitespace-nowrap ${
                  selectedIface === name ? 'bg-white text-slate-800 shadow-sm' : 'text-slate-500 hover:text-slate-700'
                }`}
              >
                <div className={`w-2 h-2 rounded-full ${info.interfaces[name]?.is_up ? 'bg-emerald-400' : 'bg-red-400'}`} />
                {name}
              </button>
            ))}
          </div>

          {ifaceData && (
            <div className="bg-white rounded-xl border border-slate-100 p-5">
              <div className="flex items-center justify-between mb-4">
                <div className="flex items-center gap-3">
                  <div className="w-10 h-10 rounded-lg bg-cyan-50 flex items-center justify-center text-cyan-500">
                    <Wifi size={18} />
                  </div>
                  <div>
                    <h3 className="font-bold text-slate-800 font-mono">{selectedIface}</h3>
                    <p className="text-xs text-slate-400">
                      {ifaceData.address.length > 0 ? ifaceData.address.join(', ') : '无 IP'}
                      {ifaceData.mac.length > 0 && ` · ${ifaceData.mac[0]}`}
                    </p>
                  </div>
                </div>
                {speeds[selectedIface] && (
                  <div className="flex items-center gap-4 text-xs">
                    <div className="text-right">
                      <p className="text-emerald-500 font-bold">{fmtS(speeds[selectedIface].down)}</p>
                      <p className="text-slate-400">下载</p>
                    </div>
                    <div className="text-right">
                      <p className="text-blue-500 font-bold">{fmtS(speeds[selectedIface].up)}</p>
                      <p className="text-slate-400">上传</p>
                    </div>
                  </div>
                )}
              </div>

              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                {[
                  ['接收流量', fmt(ifaceData.bytes_recv)],
                  ['发送流量', fmt(ifaceData.bytes_sent)],
                  ['MTU', `${ifaceData.mtu} bytes`],
                  ['状态', ifaceData.is_up ? '已连接' : '未连接'],
                  ['接收包', ifaceData.packets_recv.toLocaleString()],
                  ['发送包', ifaceData.packets_sent.toLocaleString()],
                  ['接收错误', ifaceData.errin],
                  ['发送错误', ifaceData.errout],
                ].map(([label, value]) => (
                  <div key={label} className="flex justify-between text-sm px-3 py-2 rounded-lg bg-slate-50">
                    <span className="text-slate-400">{label}</span>
                    <span className="font-medium text-slate-600">{value}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {tab === 'connections' && (
        <div className="bg-white rounded-xl border border-slate-100 p-5">
          <div className="flex gap-6 text-sm mb-6">
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-emerald-400" />
              <span className="text-slate-600">TCP <span className="font-bold text-slate-800">{info.connections.tcp}</span></span>
            </div>
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-purple-400" />
              <span className="text-slate-600">UDP <span className="font-bold text-slate-800">{info.connections.udp}</span></span>
            </div>
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-slate-300" />
              <span className="text-slate-600">总计 <span className="font-bold text-slate-800">{info.connections.total}</span></span>
            </div>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <div className="p-3 rounded-lg bg-emerald-50 text-center">
              <p className="text-2xl font-black text-emerald-600">{info.connections.tcp}</p>
              <p className="text-xs text-emerald-500 mt-1">TCP 连接</p>
            </div>
            <div className="p-3 rounded-lg bg-purple-50 text-center">
              <p className="text-2xl font-black text-purple-600">{info.connections.udp}</p>
              <p className="text-xs text-purple-500 mt-1">UDP 连接</p>
            </div>
            <div className="p-3 rounded-lg bg-slate-50 text-center">
              <p className="text-2xl font-black text-slate-600">{info.connections.total}</p>
              <p className="text-xs text-slate-400 mt-1">总连接数</p>
            </div>
          </div>
        </div>
      )}

      {tab === 'traffic' && (
        <div className="space-y-4">
          {selectedIface && speeds[selectedIface] && (
            <div className="grid grid-cols-2 gap-3">
              <div className="p-4 rounded-xl bg-emerald-50 text-center">
                <div className="flex items-center justify-center gap-2 mb-2">
                  <ArrowDown size={16} className="text-emerald-500" />
                  <span className="text-sm font-medium text-emerald-600">下载</span>
                </div>
                <p className="text-2xl font-black text-emerald-700">{fmtS(speeds[selectedIface].down)}</p>
              </div>
              <div className="p-4 rounded-xl bg-blue-50 text-center">
                <div className="flex items-center justify-center gap-2 mb-2">
                  <ArrowUp size={16} className="text-blue-500" />
                  <span className="text-sm font-medium text-blue-600">上传</span>
                </div>
                <p className="text-2xl font-black text-blue-700">{fmtS(speeds[selectedIface].up)}</p>
              </div>
            </div>
          )}

          <div className="bg-white rounded-xl border border-slate-100 p-4">
            <div className="flex items-center gap-4 mb-3 text-xs">
              <div className="flex items-center gap-1.5">
                <div className="w-6 h-0.5 bg-emerald-500" />
                <span className="text-slate-400">下载</span>
              </div>
              <div className="flex items-center gap-1.5">
                <div className="w-6 h-0.5 bg-blue-500" />
                <span className="text-slate-400">上传</span>
              </div>
              <span className="text-slate-400 ml-auto">近 60 秒</span>
            </div>
            <canvas
              ref={canvasRef}
              className="w-full h-[100px]"
              style={{ imageRendering: 'crisp-edges' }}
            />
          </div>

          {ifaceData && (
            <div className="bg-white rounded-xl border border-slate-100 p-5">
              <div className="grid grid-cols-2 gap-6">
                <div className="p-4 rounded-xl bg-emerald-50">
                  <div className="flex items-center gap-2 mb-3">
                    <ArrowDown size={16} className="text-emerald-500" />
                    <span className="text-sm font-medium text-emerald-600">下载</span>
                  </div>
                  <p className="text-2xl font-black text-emerald-700">{fmt(ifaceData.bytes_recv)}</p>
                  <p className="text-xs text-emerald-500 mt-1">{ifaceData.packets_recv.toLocaleString()} 包</p>
                </div>
                <div className="p-4 rounded-xl bg-blue-50">
                  <div className="flex items-center gap-2 mb-3">
                    <ArrowUp size={16} className="text-blue-500" />
                    <span className="text-sm font-medium text-blue-600">上传</span>
                  </div>
                  <p className="text-2xl font-black text-blue-700">{fmt(ifaceData.bytes_sent)}</p>
                  <p className="text-xs text-blue-500 mt-1">{ifaceData.packets_sent.toLocaleString()} 包</p>
                </div>
              </div>
            </div>
          )}
        </div>
      )}

      {tab === 'ports' && (
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-3">
            <div className="bg-white p-4 rounded-xl border border-slate-100 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-cyan-50 flex items-center justify-center text-cyan-500">
                <Radio size={18} />
              </div>
              <div>
                <p className="text-2xl font-black text-slate-800">{info.listening_ports.length}</p>
                <p className="text-xs text-slate-400">监听端口</p>
              </div>
            </div>
            <div className="bg-white p-4 rounded-xl border border-slate-100 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-orange-50 flex items-center justify-center text-orange-500">
                <Link size={18} />
              </div>
              <div>
                <p className="text-2xl font-black text-slate-800">{info.active_connections.length}</p>
                <p className="text-xs text-slate-400">活跃连接</p>
              </div>
            </div>
          </div>

          <div className="bg-white rounded-xl border border-slate-100 overflow-hidden">
            <div className="px-5 py-3 border-b border-slate-100">
              <h3 className="font-bold text-slate-800 text-sm">监听端口</h3>
            </div>
            <div className="overflow-x-auto">
              <table className="min-w-full">
                <thead>
                  <tr className="border-b border-slate-50 text-slate-400 text-xs uppercase tracking-wider font-semibold">
                    <th className="py-3 pl-5 text-left">协议</th>
                    <th className="py-3 text-left">端口</th>
                    <th className="py-3 text-left">地址</th>
                    <th className="py-3 text-left">进程</th>
                    <th className="py-3 text-left">PID</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-50">
                  {info.listening_ports.sort((a, b) => a.port - b.port).map((p, idx) => (
                    <tr key={idx} className="hover:bg-slate-50/50 transition-colors">
                      <td className="py-3 pl-5">
                        <span className={`text-xs px-2 py-0.5 rounded font-bold ${
                          p.protocol === 'TCP' ? 'bg-emerald-50 text-emerald-600' : 'bg-purple-50 text-purple-600'
                        }`}>
                          {p.protocol}
                        </span>
                      </td>
                      <td className="py-3">
                        <span className="text-sm font-mono font-bold text-slate-700">{p.port}</span>
                      </td>
                      <td className="py-3">
                        <span className="text-xs text-slate-400 font-mono">{p.address}</span>
                      </td>
                      <td className="py-3">
                        <span className="text-xs text-slate-600">{p.process || '-'}</span>
                      </td>
                      <td className="py-3">
                        <span className="text-xs text-slate-400 font-mono">{p.pid || '-'}</span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="px-5 py-2 border-t border-slate-50 text-xs text-slate-400">
              共 {info.listening_ports.length} 个监听端口
            </div>
          </div>

          {info.active_connections.length > 0 && (
            <div className="bg-white rounded-xl border border-slate-100 overflow-hidden">
              <div className="px-5 py-3 border-b border-slate-100">
                <h3 className="font-bold text-slate-800 text-sm">活跃连接</h3>
              </div>
              <div className="overflow-x-auto max-h-96">
                <table className="min-w-full">
                  <thead>
                    <tr className="border-b border-slate-50 text-slate-400 text-xs uppercase tracking-wider font-semibold">
                      <th className="py-3 pl-5 text-left">协议</th>
                      <th className="py-3 text-left">本地地址</th>
                      <th className="py-3 text-left">远程地址</th>
                      <th className="py-3 text-left">状态</th>
                      <th className="py-3 text-left">PID</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-50">
                    {info.active_connections.slice(0, 200).map((c, idx) => (
                      <tr key={idx} className="hover:bg-slate-50/50 transition-colors">
                        <td className="py-2 pl-5">
                          <span className={`text-xs px-2 py-0.5 rounded font-bold ${
                            c.protocol === 'TCP' ? 'bg-emerald-50 text-emerald-600' : 'bg-purple-50 text-purple-600'
                          }`}>
                            {c.protocol}
                          </span>
                        </td>
                        <td className="py-2">
                          <span className="text-xs font-mono text-slate-600">{c.laddr}</span>
                        </td>
                        <td className="py-2">
                          <span className="text-xs font-mono text-slate-400">{c.raddr}</span>
                        </td>
                        <td className="py-2">
                          <span className="text-xs text-slate-400">{c.status}</span>
                        </td>
                        <td className="py-2">
                          <span className="text-xs text-slate-400 font-mono">{c.pid || '-'}</span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <div className="px-5 py-2 border-t border-slate-50 text-xs text-slate-400">
                共 {info.active_connections.length} 个活跃连接（显示前 200 条）
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
