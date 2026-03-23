import React, { useEffect, useState, useCallback } from 'react';
import { 
  Server,
  Activity,
  Cpu,
  HardDrive,
  Terminal,
  Shield,
  Zap,
  Network,
  Search,
  LayoutDashboard,
  Container,
  Activity as ActivityIcon,
  RotateCcw
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { Card } from './components/Card';
import { MetricChart } from './components/MetricChart';
import { RadialMetric } from './components/RadialMetric';
import { DockerGrid } from './components/DockerGrid';
import { ProcessTable } from './components/ProcessTable';
import { StatsHero } from './components/StatsHero';
import { cn } from './lib/utils';
import './App.css';

const API_BASE = 'http://localhost:8000/api';

const NAV_TABS = [
  { id: 'overview', icon: LayoutDashboard, label: 'Overview' },
  { id: 'containers', icon: Container, label: 'Containers' },
  { id: 'processes', icon: ActivityIcon, label: 'Processes' }
];

const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<'overview' | 'containers' | 'processes'>('overview');
  const [systemInfo, setSystemInfo] = useState<any>(null);
  const [dockerInfo, setDockerInfo] = useState<any>([]);
  const [processes, setProcesses] = useState<any>([]);
  const [startup, setStartup] = useState<any>(null);
  const [loading, setLoading] = useState(true);
  const [history, setHistory] = useState<any[]>([]);
  const [searchTerm, setSearchTerm] = useState('');

  const formatBytes = (bytes?: number) => {
    if (!bytes || bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  };

  const fetchData = useCallback(async () => {
    try {
      const [sys, dock, proc, start] = await Promise.all([
        fetch(`${API_BASE}/system/info`).then(res => res.json()),
        fetch(`${API_BASE}/system/docker`).then(res => res.json()),
        fetch(`${API_BASE}/system/processes?limit=25`).then(res => res.json()),
        fetch(`${API_BASE}/system/startup`).then(res => res.json())
      ]);

      setSystemInfo(sys);
      setDockerInfo(Array.isArray(dock) ? dock : []);
      setProcesses(proc);
      setStartup(start);

      setHistory(prev => {
        const next = [...prev, {
          time: new Date().toLocaleTimeString(),
          cpu: sys.cpu_percent || 0,
          memory: sys.memory?.percent || 0
        }].slice(-30);
        return next;
      });

      setLoading(false);
    } catch (err) {
      console.error('Dashboard error', err);
    }
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 3000);
    return () => clearInterval(interval);
  }, [fetchData]);

  if (loading) return (
    <div className="flex flex-col items-center justify-center h-screen bg-[#fbfbfd] text-blue-500 gap-6">
      <motion.div
        animate={{ scale: [1, 1.1, 1], opacity: [0.5, 1, 0.5] }}
        transition={{ repeat: Infinity, duration: 1.5 }}
        className="relative"
      >
        <div className="absolute inset-0 blur-2xl bg-blue-500/20 rounded-full" />
        <Server size={64} className="relative z-10 text-blue-600" />
      </motion.div>
      <div className="font-mono text-[10px] uppercase tracking-[0.4em] font-black text-gray-400">Initialize System Link...</div>
    </div>
  );

  const cpuPercent = typeof systemInfo?.cpu_percent === 'number' ? `${systemInfo.cpu_percent.toFixed(1)}%` : '--';
  const freqInfo = typeof systemInfo?.cpu_freq?.current === 'number' ? `${systemInfo.cpu_freq.current.toFixed(0)} MHz` : 'freq unknown';
  const memoryPercent = typeof systemInfo?.memory?.percent === 'number' ? `${systemInfo.memory.percent.toFixed(1)}%` : '--';
  const memoryUsed = formatBytes(systemInfo?.memory?.used);
  const memoryTotal = formatBytes(systemInfo?.memory?.total);
  const diskPercent = typeof systemInfo?.disk?.percent === 'number' ? `${systemInfo.disk.percent.toFixed(1)}%` : '--';
  const networkTotal = systemInfo?.network ? formatBytes(systemInfo.network.bytes_sent + systemInfo.network.bytes_recv) : '--';
  const formattedUptime = systemInfo ? `${Math.floor(systemInfo.uptime_seconds / 3600)}h ${Math.floor((systemInfo.uptime_seconds % 3600) / 60)}m` : '--';
  const load1 = startup?.load_avg?.[0]?.toFixed(2) ?? '--';
  const load15 = startup?.load_avg?.[2]?.toFixed(2) ?? '--';
  const lastUpdate = history[history.length - 1]?.time ?? '--';

  const highlightChips = [
    { label: 'CPU Load', value: cpuPercent, detail: freqInfo, icon: <Cpu size={14} /> },
    { label: 'Memory', value: memoryPercent, detail: `${memoryUsed} / ${memoryTotal}`, icon: <Activity size={14} /> },
    { label: 'Network', value: networkTotal, detail: 'Total throughput', icon: <Network size={14} /> }
  ];

  const signalChips = [
    { label: 'Active Containers', value: dockerInfo.length, detail: 'running or paused' },
    { label: 'Tracked Processes', value: processes.length, detail: 'ordered by CPU' },
    { label: 'Last Sync', value: lastUpdate, detail: 'auto refreshed' }
  ];

  return (
    <div className="dashboard-shell">
      <StatsHero 
        activeTab={activeTab}
        setActiveTab={setActiveTab}
        navTabs={NAV_TABS}
        highlightChips={highlightChips}
        signalChips={signalChips}
      />

      <main className="relative z-10 max-w-6xl mx-auto px-6 pb-16 space-y-8">
        <AnimatePresence mode="wait">
          {activeTab === 'overview' && (
            <motion.section
              initial={{ opacity: 0, scale: 0.98 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.98 }}
              className="space-y-8"
              key="overview"
            >
              <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
                <Card className="panel-glass">
                  <RadialMetric value={systemInfo?.cpu_percent || 0} label="CPU Load" color="#3b82f6" subValue={`${systemInfo?.cpu_count} Cores`} icon={<Cpu size={14} />} />
                  <MetricChart data={history} dataKey="cpu" color="#3b82f6" />
                </Card>

                <Card className="panel-glass">
                  <RadialMetric value={systemInfo?.memory?.percent || 0} label="Memory" color="#10b981" subValue={formatBytes(systemInfo?.memory?.used)} icon={<Activity size={14} />} />
                  <MetricChart data={history} dataKey="memory" color="#10b981" />
                </Card>

                <Card className="panel-glass flex flex-col justify-between gap-5">
                  <div>
                    <div className="flex items-center justify-between pb-4 border-b border-gray-100/50">
                      <div>
                        <p className="text-[10px] uppercase font-black tracking-widest text-gray-400">Throughput</p>
                        <h3 className="text-3xl font-black text-gray-900 mt-1">{networkTotal}</h3>
                      </div>
                      <div className="p-3 bg-blue-50 rounded-2xl text-blue-600">
                        <Network size={20} />
                      </div>
                    </div>
                    <div className="mt-8 space-y-6">
                      <div className="space-y-2">
                        <div className="flex items-center justify-between text-xs font-bold text-gray-500">
                          <span>TX (Sent)</span>
                          <span className="text-gray-900">{formatBytes(systemInfo?.network?.bytes_sent)}</span>
                        </div>
                        <div className="w-full bg-gray-100 h-1.5 rounded-full overflow-hidden">
                          <motion.div
                            className="h-full bg-blue-600"
                            initial={{ width: 0 }}
                            animate={{ width: `${systemInfo?.network?.bytes_sent ? Math.min((systemInfo.network.bytes_sent / 1e8) * 100, 100) : 0}%` }}
                          />
                        </div>
                      </div>
                      <div className="space-y-2">
                        <div className="flex items-center justify-between text-xs font-bold text-gray-500">
                          <span>RX (Received)</span>
                          <span className="text-gray-900">{formatBytes(systemInfo?.network?.bytes_recv)}</span>
                        </div>
                        <div className="w-full bg-gray-100 h-1.5 rounded-full overflow-hidden">
                          <motion.div
                            className="h-full bg-emerald-500"
                            initial={{ width: 0 }}
                            animate={{ width: `${systemInfo?.network?.bytes_recv ? Math.min((systemInfo.network.bytes_recv / 1e8) * 100, 100) : 0}%` }}
                          />
                        </div>
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center justify-between text-[10px] font-black uppercase tracking-[0.2em] text-gray-400 mt-4">
                    <span>Load (1m / 15m)</span>
                    <span className="text-gray-900">{load1} / {load15}</span>
                  </div>
                </Card>
              </div>

              <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
                <Card className="panel-glass">
                  <div className="flex items-center justify-between pb-6 border-b border-gray-100/50">
                    <div>
                      <p className="text-[10px] uppercase font-black tracking-widest text-gray-400">Storage Efficiency</p>
                      <h3 className="text-2xl font-black text-gray-900 mt-1">Main Volume: {diskPercent}</h3>
                    </div>
                    <div className="p-3 bg-indigo-50 rounded-2xl text-indigo-600">
                      <HardDrive size={20} />
                    </div>
                  </div>
                  <div className="mt-8 space-y-6">
                    <div className="h-2 w-full bg-gray-100 rounded-full overflow-hidden">
                      <motion.div
                        className="h-full bg-gradient-to-r from-blue-600 to-indigo-500"
                        animate={{ width: `${systemInfo?.disk?.percent ?? 0}%` }}
                      />
                    </div>
                    <div className="grid grid-cols-2 gap-6">
                      <div className="space-y-1">
                        <p className="text-[10px] uppercase font-black text-gray-400 tracking-tighter">Usage</p>
                        <p className="text-lg font-bold text-gray-900">{formatBytes(systemInfo?.disk?.used)}</p>
                      </div>
                      <div className="space-y-1">
                        <p className="text-[10px] uppercase font-black text-gray-400 tracking-tighter">Full Capacity</p>
                        <p className="text-lg font-bold text-gray-400">{formatBytes(systemInfo?.disk?.total)}</p>
                      </div>
                    </div>
                  </div>
                </Card>

                <Card className="panel-glass flex flex-col justify-between">
                  <div className="flex items-center justify-between">
                    <div>
                      <p className="text-[10px] uppercase font-black tracking-widest text-gray-400">Operational Health</p>
                      <h3 className="text-3xl font-black text-gray-900 mt-1 uppercase">{formattedUptime}</h3>
                      <p className="text-xs font-bold text-emerald-500 mt-1 uppercase tracking-tighter">Verified Uptime Record</p>
                    </div>
                    <div className="p-4 bg-emerald-50 rounded-[20px] text-emerald-600">
                      <Shield size={32} />
                    </div>
                  </div>
                  <div className="mt-8 space-y-4 text-xs font-bold text-gray-500 uppercase tracking-tight">
                    <div className="flex justify-between"><span>Boot timestamp</span> <span className="text-gray-900">{systemInfo?.boot_time}</span></div>
                    <div className="flex justify-between"><span>Authorized Sessions</span> <span className="text-gray-900">{startup?.users?.length ?? 0}</span></div>
                    <div className="flex justify-between"><span>Last Handshake</span> <span className="text-gray-900">{lastUpdate}</span></div>
                  </div>
                </Card>
              </div>

              <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
                <Card className="panel-glass">
                  <div className="flex items-center justify-between mb-8">
                    <h3 className="text-xl font-black text-gray-900 flex items-center gap-3 underline decoration-blue-600/30 decoration-4 underline-offset-4"><Container size={20} className="text-blue-600" /> Docker Snapshot</h3>
                    <div className="px-3 py-1 bg-gray-100 rounded-full text-[10px] font-black text-gray-500 uppercase">Live Count: {dockerInfo.length}</div>
                  </div>
                  <DockerGrid containers={dockerInfo.slice(0, 4)} onRefresh={fetchData} />
                </Card>

                <Card className="panel-glass">
                  <div className="flex items-center justify-between mb-8">
                    <h3 className="text-xl font-black text-gray-900 flex items-center gap-3 underline decoration-indigo-600/30 decoration-4 underline-offset-4"><Terminal size={20} className="text-indigo-600" /> High Activity</h3>
                    <div className="px-3 py-1 bg-gray-100 rounded-full text-[10px] font-black text-gray-500 uppercase">Core Limit: 6</div>
                  </div>
                  <div className="space-y-4">
                    {processes.slice(0, 6).map((proc: any) => (
                      <div key={proc.pid} className="flex items-center justify-between p-4 bg-gray-50/50 hover:bg-white rounded-2xl border border-transparent hover:border-gray-200 transition-all duration-200 group">
                        <div className="flex items-center gap-4">
                          <div className="w-10 h-10 rounded-xl bg-white border border-gray-100 flex items-center justify-center font-black text-blue-600 text-xs shadow-sm">
                            {proc.name.charAt(0).toUpperCase()}
                          </div>
                          <div>
                            <p className="text-sm font-black text-gray-900 leading-none">{proc.name}</p>
                            <p className="text-[10px] uppercase font-bold text-gray-400 mt-1">ID: {proc.pid}</p>
                          </div>
                        </div>
                        <div className="flex items-center gap-6">
                          <div className="text-right">
                            <p className="text-[10px] uppercase font-bold text-gray-400">Load</p>
                            <p className="text-xs font-black text-gray-900">{proc.cpu_percent.toFixed(1)}%</p>
                          </div>
                          <div className="text-right">
                            <p className="text-[10px] uppercase font-bold text-gray-400">Memory</p>
                            <p className="text-xs font-black text-gray-900">{proc.memory_percent.toFixed(1)}%</p>
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                </Card>
              </div>
            </motion.section>
          )}

          {activeTab === 'containers' && (
            <motion.section
              key="containers"
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -12 }}
              className="space-y-8"
            >
              <div className="flex items-center justify-between p-6 bg-white border border-gray-100 rounded-[32px] shadow-sm">
                <div>
                  <p className="text-[10px] uppercase font-black tracking-[0.3em] text-blue-600 mb-1">Docker Operations</p>
                  <h2 className="text-3xl font-black text-gray-900">Virtual Isolation</h2>
                </div>
                <button onClick={fetchData} className="px-6 py-3 rounded-2xl bg-gray-900 text-white text-xs font-black uppercase tracking-widest hover:bg-gray-800 transition shadow-lg shadow-gray-200">Re-Sync Environment</button>
              </div>
              <div className="panel-glass">
                <DockerGrid containers={dockerInfo} onRefresh={fetchData} />
              </div>
            </motion.section>
          )}

          {activeTab === 'processes' && (
            <ProcessTable 
              processes={processes}
              searchTerm={searchTerm}
              onSearchChange={setSearchTerm}
            />
          )}
        </AnimatePresence>
      </main>
    </div>
  );
};

export default App;
