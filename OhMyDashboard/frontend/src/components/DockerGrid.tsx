import React from 'react';
import { 
  Play, 
  Square, 
  RotateCw, 
  Activity, 
  Box, 
  Cpu, 
  Database 
} from 'lucide-react';
import { cn } from '../lib/utils';

interface Container {
  id: string;
  name: string;
  image: string;
  status: string;
  state: string;
  cpu_percent: number;
  memory_usage: number;
  memory_limit: number;
}

interface DockerGridProps {
  containers: Container[];
  onRefresh: () => void;
}

const API_BASE = 'http://localhost:8000/api';

export const DockerGrid: React.FC<DockerGridProps> = ({ containers, onRefresh }) => {
  const controlContainer = async (id: string, action: string) => {
    try {
      await fetch(`${API_BASE}/system/docker/control`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ container_id: id, action })
      });
      onRefresh();
    } catch (err) {
      console.error('Docker action failed', err);
    }
  };

  const formatBytes = (bytes?: number) => {
    if (!bytes || bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  };

  if (!Array.isArray(containers) || containers.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-gray-400 bg-gray-50/50 rounded-[20px] border border-dashed border-gray-200">
        <Box size={40} className="mb-4 opacity-20" />
        <p className="text-sm font-bold uppercase tracking-widest">No Active Containers</p>
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
      {containers.map((container) => (
        <div key={container.id} className="p-5 bg-white border border-gray-100 rounded-[24px] shadow-sm hover:shadow-md transition-shadow">
          <div className="flex items-start justify-between mb-6">
            <div className="flex items-center gap-4">
              <div className={cn(
                "w-10 h-10 rounded-xl flex items-center justify-center",
                container.state === 'running' ? "bg-blue-50 text-blue-600" : "bg-gray-100 text-gray-400"
              )}>
                <Box size={20} />
              </div>
              <div>
                <h4 className="font-black text-gray-900 leading-tight">{container.name}</h4>
                <p className="text-[10px] font-bold text-gray-400 uppercase tracking-tighter truncate max-w-[120px]">{container.image}</p>
              </div>
            </div>
            <div className="flex gap-1">
              {container.state === 'running' ? (
                <button 
                  onClick={() => controlContainer(container.id, 'stop')}
                  className="p-2 hover:bg-red-50 text-gray-400 hover:text-red-500 transition-colors rounded-xl"
                >
                  <Square size={16} fill="currentColor" />
                </button>
              ) : (
                <button 
                  onClick={() => controlContainer(container.id, 'start')}
                  className="p-2 hover:bg-blue-50 text-gray-400 hover:text-blue-600 transition-colors rounded-xl"
                >
                  <Play size={16} fill="currentColor" />
                </button>
              )}
              <button 
                onClick={() => controlContainer(container.id, 'restart')}
                className="p-2 hover:bg-gray-100 text-gray-400 hover:text-gray-900 transition-colors rounded-xl"
              >
                <RotateCw size={16} />
              </button>
            </div>
          </div>

          <div className="flex items-center gap-6 mt-4">
            <div className="flex-1">
              <div className="flex justify-between text-[10px] font-black text-gray-400 uppercase mb-2">
                <span>CPU Load</span>
                <span className="text-gray-900">{container.cpu_percent.toFixed(1)}%</span>
              </div>
              <div className="h-1.5 w-full bg-gray-100 rounded-full overflow-hidden">
                <div 
                  className="h-full bg-blue-600 transition-all duration-500" 
                  style={{ width: `${Math.min(container.cpu_percent, 100)}%` }} 
                />
              </div>
            </div>
            <div className="flex-1">
              <div className="flex justify-between text-[10px] font-black text-gray-400 uppercase mb-2">
                <span>Memory</span>
                <span className="text-gray-900">{formatBytes(container.memory_usage)}</span>
              </div>
              <div className="h-1.5 w-full bg-gray-100 rounded-full overflow-hidden">
                <div 
                  className="h-full bg-emerald-500 transition-all duration-500" 
                  style={{ width: `${(container.memory_usage / container.memory_limit) * 100}%` }} 
                />
              </div>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
};
