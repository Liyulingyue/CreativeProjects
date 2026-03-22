import React, { useEffect, useState } from 'react';
import { 
  Server, 
  Box, 
  Activity, 
  Cpu, 
  HardDrive, 
  Clock, 
  Users,
  Terminal
} from 'lucide-react';

const API_BASE = 'http://localhost:8000/api';

const App: React.FC = () => {
  const [systemInfo, setSystemInfo] = useState<any>(null);
  const [dockerInfo, setDockerInfo] = useState<any>([]);
  const [processes, setProcesses] = useState<any>([]);
  const [startup, setStartup] = useState<any>(null);
  const [loading, setLoading] = useState(true);

  const fetchData = async () => {
    try {
      const [sys, dock, proc, start] = await Promise.all([
        fetch(`${API_BASE}/system/info`).then(res => res.json()),
        fetch(`${API_BASE}/system/docker`).then(res => res.json()),
        fetch(`${API_BASE}/system/processes?limit=10`).then(res => res.json()),
        fetch(`${API_BASE}/system/startup`).then(res => res.json())
      ]);
      setSystemInfo(sys);
      setDockerInfo(dock);
      setProcesses(proc);
      setStartup(start);
    } catch (err) {
      console.error("Failed to fetch dashboard data", err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
    const timer = setInterval(fetchData, 5000);
    return () => clearInterval(timer);
  }, []);

  if (loading) return <div className="flex items-center justify-center h-screen text-slate-400">Loading Dashboard...</div>;

  return (
    <div className="min-h-screen bg-slate-950 p-6 text-slate-200 w-full text-left">
      {/* Header */}
      <div className="mb-8 flex items-center justify-between">
        <h1 className="text-3xl font-bold flex items-center gap-3 text-white !m-0">
          <Server className="text-blue-500" /> OhMyDashboard
        </h1>
        <div className="text-slate-400 text-sm">
          Last updated: {new Date().toLocaleTimeString()}
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
        {/* System Overview Cards */}
        <div className="bg-slate-900 border border-slate-800 p-5 rounded-xl">
          <div className="flex items-center gap-3 text-slate-400 mb-2">
            <Cpu size={20} /> CPU Load
          </div>
          <div className="text-2xl font-mono text-blue-400">{systemInfo?.cpu_freq?.current?.toFixed(0)} MHz</div>
          <div className="text-xs text-slate-500 mt-1 whitespace-pre-wrap">{systemInfo?.cpu_count} Cores / {systemInfo?.os}</div>
        </div>

        <div className="bg-slate-900 border border-slate-800 p-5 rounded-xl">
          <div className="flex items-center gap-3 text-slate-400 mb-2">
            <Activity size={20} /> Memory
          </div>
          <div className="text-2xl font-mono text-green-400">
            {(systemInfo?.memory?.percent)?.toFixed(1)}%
          </div>
          <div className="text-xs text-slate-500 mt-1">
            Used: {(systemInfo?.memory?.used / 1024**3).toFixed(1)}GB / Total: {(systemInfo?.memory?.total / 1024**3).toFixed(1)}GB
          </div>
        </div>

        <div className="bg-slate-900 border border-slate-800 p-5 rounded-xl">
          <div className="flex items-center gap-3 text-slate-400 mb-2">
            <Clock size={20} /> Uptime
          </div>
          <div className="text-sm font-mono text-orange-400">
            {new Date(systemInfo?.boot_time).toLocaleString()}
          </div>
          <div className="text-xs text-slate-500 mt-1">Since system boot</div>
        </div>

        <div className="bg-slate-900 border border-slate-800 p-5 rounded-xl">
          <div className="flex items-center gap-3 text-slate-400 mb-2">
            <Activity size={20} /> Load Avg
          </div>
          <div className="text-xl font-mono text-purple-400">
            {startup?.load_avg?.map((v: number) => v.toFixed(2)).join(' / ')}
          </div>
          <div className="text-xs text-slate-500 mt-1 flex items-center gap-1">
             <Users size={12} /> {startup?.users?.length} Active Users
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Docker Status */}
        <div className="lg:col-span-1 bg-slate-900 border border-slate-800 rounded-xl overflow-hidden">
          <div className="bg-slate-800/50 p-4 font-semibold flex items-center gap-2">
            <Box size={18} className="text-blue-400" /> Docker Containers
          </div>
          <div className="p-4 space-y-3">
            {Array.isArray(dockerInfo) && dockerInfo.length > 0 ? dockerInfo.map((container: any) => (
              <div key={container.id} className="flex items-center justify-between p-2 rounded bg-slate-800/30 border border-slate-800/50 transition hover:bg-slate-800/50">
                <div className="truncate pr-4">
                  <div className="font-medium text-sm text-slate-200">{container.name}</div>
                  <div className="text-[10px] text-slate-500 truncate">{container.image}</div>
                </div>
                <div className={`text-[10px] px-2 py-0.5 rounded-full font-bold uppercase ${
                  container.status === 'running' ? 'bg-green-500/10 text-green-500' : 'bg-red-500/10 text-red-500'
                }`}>
                  {container.status}
                </div>
              </div>
            )) : <div className="text-slate-600 text-sm">No active docker containers.</div>}
          </div>
        </div>

        {/* Process List */}
        <div className="lg:col-span-2 bg-slate-900 border border-slate-800 rounded-xl overflow-hidden">
          <div className="bg-slate-800/50 p-4 font-semibold flex items-center gap-2">
            <Terminal size={18} className="text-green-400" /> Top Processes (CPU %)
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead className="bg-slate-950/50 text-slate-500 border-b border-slate-800">
                <tr>
                  <th className="p-3 font-medium">PID</th>
                  <th className="p-3 font-medium">Name</th>
                  <th className="p-3 font-medium text-right">CPU %</th>
                  <th className="p-3 font-medium text-right">MEM %</th>
                  <th className="p-3 font-medium">User</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/50">
                {processes.map((proc: any) => (
                  <tr key={proc.pid} className="hover:bg-slate-800/20">
                    <td className="p-3 font-mono text-slate-500">{proc.pid}</td>
                    <td className="p-3 font-medium text-slate-200 truncate max-w-[150px]">{proc.name}</td>
                    <td className="p-3 text-right">
                      <span className={`px-2 py-1 rounded-md text-xs font-mono font-bold ${
                        proc.cpu_percent > 20 ? 'text-red-400 bg-red-400/10' : 'text-blue-400 bg-blue-400/10'
                      }`}>
                        {proc.cpu_percent.toFixed(1)}%
                      </span>
                    </td>
                    <td className="p-3 text-right text-slate-400 font-mono">{proc.memory_percent.toFixed(1)}%</td>
                    <td className="p-3 text-slate-500">{proc.username}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  );
};

export default App;
