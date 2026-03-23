import React, { useState } from 'react';
import { Box, Play, Square, RotateCcw, Image } from 'lucide-react';

interface ContainerActionProps {
  id: string;
  name: string;
  image: string;
  status: string;
  onRefresh: () => void;
}

export const DockerCard: React.FC<ContainerActionProps> = ({ id, name, image, status, onRefresh }) => {
  const [loading, setLoading] = useState(false);

  const handleAction = async (action: string) => {
    setLoading(true);
    try {
      await fetch(`http://localhost:8000/api/system/docker/${id}/${action}`, { method: 'POST' });
      onRefresh();
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  };

  const isRunning = status === 'running';

  return (
    <div className="flex items-center justify-between p-3 rounded-xl bg-slate-900/30 border border-slate-800/50 hover:border-slate-700 transition group mb-3 last:mb-0">
      <div className="flex items-center gap-3 overflow-hidden">
        <div className={`p-2 rounded-lg ${isRunning ? 'bg-green-500/10 text-green-500' : 'bg-red-500/10 text-red-500'}`}>
          <Box size={18} />
        </div>
        <div className="overflow-hidden">
          <div className="font-semibold text-slate-200 truncate">{name}</div>
          <div className="text-[10px] text-slate-500 flex items-center gap-1">
            <Image size={10} /> {image}
          </div>
        </div>
      </div>
      
      <div className="flex items-center gap-2">
         <span className={`text-[10px] px-2 py-0.5 rounded-full font-bold uppercase ${
          isRunning ? 'bg-green-500/10 text-green-500' : 'bg-red-500/10 text-red-500'
        }`}>
          {status}
        </span>
        <div className="flex gap-1 border-l border-slate-700 pl-2 ml-1">
          {isRunning ? (
            <button 
              onClick={() => handleAction('stop')} 
              disabled={loading}
              className="p-1.5 hover:bg-red-500/10 text-slate-400 hover:text-red-400 rounded-lg transition"
              title="Stop"
            >
              <Square size={14} />
            </button>
          ) : (
            <button 
              onClick={() => handleAction('start')} 
              disabled={loading}
              className="p-1.5 hover:bg-green-500/10 text-slate-400 hover:text-green-400 rounded-lg transition"
              title="Start"
            >
              <Play size={14} />
            </button>
          )}
          <button 
            onClick={() => handleAction('restart')} 
            disabled={loading}
            className="p-1.5 hover:bg-blue-500/10 text-slate-400 hover:text-blue-400 rounded-lg transition"
            title="Restart"
          >
            <RotateCcw size={14} />
          </button>
        </div>
      </div>
    </div>
  );
};
