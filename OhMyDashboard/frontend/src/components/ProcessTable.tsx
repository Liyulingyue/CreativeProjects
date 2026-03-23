import React from 'react';
import { Search, Activity, Cpu, User } from 'lucide-react';
import { motion } from 'framer-motion';
import { cn } from '../lib/utils';

interface Process {
  pid: number;
  name: string;
  username: string;
  status: string;
  cpu_percent: number;
  memory_percent: number;
}

interface ProcessTableProps {
  processes: Process[];
  searchTerm: string;
  onSearchChange: (value: string) => void;
}

export const ProcessTable: React.FC<ProcessTableProps> = ({ 
  processes, 
  searchTerm, 
  onSearchChange 
}) => {
  const filteredProcesses = processes.filter((p) =>
    p.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
    p.pid.toString().includes(searchTerm)
  );

  return (
    <motion.section
      initial={{ opacity: 0, y: 15 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -15 }}
      className="space-y-8"
    >
      <div className="flex flex-col gap-6 md:flex-row md:items-center md:justify-between p-8 bg-white/80 backdrop-blur-xl border border-white rounded-[32px] shadow-sm">
        <div>
          <p className="text-[10px] uppercase font-black tracking-[0.3em] text-blue-600 mb-1">Process Analytics</p>
          <h2 className="text-3xl font-black text-gray-900 tracking-tight">System Monitor 01</h2>
        </div>
        <div className="relative group">
          <Search className="absolute left-5 top-1/2 -translate-y-1/2 text-gray-400 group-focus-within:text-blue-500 transition-colors" size={20} />
          <input
            type="text"
            placeholder="Search PID or name..."
            className="bg-gray-50 border border-gray-100 rounded-2xl pl-12 pr-6 py-4 w-80 text-sm font-bold placeholder:text-gray-300 focus:bg-white focus:border-blue-500/30 outline-none transition-all shadow-inner"
            value={searchTerm}
            onChange={(e) => onSearchChange(e.target.value)}
          />
        </div>
      </div>

      <div className="panel-glass overflow-hidden p-0 rounded-[32px] border-white shadow-xl">
        <div className="overflow-x-auto">
          <table className="min-w-full text-left">
            <thead className="bg-gray-50/50">
              <tr className="border-b border-gray-100">
                <th className="px-10 py-5 text-[10px] uppercase font-black tracking-widest text-gray-400">Process Affinity</th>
                <th className="px-6 py-5 text-[10px] uppercase font-black tracking-widest text-gray-400 text-center"><User size={14} className="inline mr-1" /> Owner</th>
                <th className="px-6 py-5 text-[10px] uppercase font-black tracking-widest text-gray-400 text-center">Status</th>
                <th className="px-6 py-5 text-right text-[10px] uppercase font-black tracking-widest text-gray-400 px-10">Resource Utilization</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-50">
              {filteredProcesses.map((proc) => (
                <tr key={proc.pid} className="hover:bg-blue-50/30 transition-colors group">
                  <td className="px-10 py-6">
                    <div className="flex items-center gap-4">
                      <div className="w-10 h-10 rounded-xl bg-gray-50 border border-gray-100 flex items-center justify-center font-black text-gray-400 text-xs group-hover:bg-white group-hover:text-blue-600 group-hover:border-blue-100 transition-all">
                        {proc.name.charAt(0).toUpperCase()}
                      </div>
                      <div>
                        <div className="font-black text-gray-900 text-sm">{proc.name}</div>
                        <div className="text-[10px] font-bold text-gray-400 uppercase tracking-tighter mt-0.5">PID: {proc.pid}</div>
                      </div>
                    </div>
                  </td>
                  <td className="px-6 py-6 text-center">
                    <span className="text-xs font-bold text-gray-500 bg-gray-100 rounded-lg px-2 py-1">{proc.username}</span>
                  </td>
                  <td className="px-6 py-6 text-center">
                    <span className={cn(
                      'px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest border transition-all',
                      proc.status === 'running' 
                        ? 'bg-blue-50 text-blue-600 border-blue-100' 
                        : 'bg-gray-50 text-gray-400 border-gray-100'
                    )}>
                      {proc.status}
                    </span>
                  </td>
                  <td className="px-10 py-6 text-right">
                    <div className="inline-flex gap-6 items-center">
                      <div className="text-right">
                        <p className="text-[10px] font-black text-gray-300 uppercase leading-none mb-1">CPU</p>
                        <p className="text-sm font-black text-gray-900">{proc.cpu_percent.toFixed(1)}%</p>
                      </div>
                      <div className="text-right border-l border-gray-100 pl-6">
                        <p className="text-[10px] font-black text-gray-300 uppercase leading-none mb-1">MEM</p>
                        <p className="text-sm font-black text-gray-900">{proc.memory_percent.toFixed(1)}%</p>
                      </div>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </motion.section>
  );
};
