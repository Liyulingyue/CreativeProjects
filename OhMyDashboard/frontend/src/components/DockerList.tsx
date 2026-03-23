import React, { useEffect, useState } from 'react';
import { api } from '../api';
import type { DockerContainer } from '../types.ts';
import { Box, Play, Square, RotateCcw, AlertTriangle, Layers } from 'lucide-react';

export const DockerList: React.FC = () => {
    const [containers, setContainers] = useState<DockerContainer[]>([]);
    const [isLoading, setIsLoading] = useState(true);

    const fetchDocker = async () => {
        const data = await api.getDockerInfo();
        if (Array.isArray(data)) {
            setContainers(data);
        }
        setIsLoading(false);
    };

    useEffect(() => {
        fetchDocker();
        const timer = setInterval(fetchDocker, 10000);
        return () => clearInterval(timer);
    }, []);

    const handleAction = async (id: string, action: 'start'|'stop'|'restart') => {
        await api.manageDocker(id, action);
        fetchDocker();
    };

    return (
        <div className="bg-white p-6 rounded-2xl shadow-sm border border-slate-100 flex-1">
            <div className="flex items-center justify-between mb-6">
                <div className="flex items-center gap-2">
                    <Layers size={20} className="text-blue-600" />
                    <h2 className="text-lg font-bold text-slate-800">Docker 容器</h2>
                </div>
                <span className="text-xs font-mono px-2 py-1 bg-slate-100 text-slate-500 rounded-full">
                    {containers.length} 个实例
                </span>
            </div>
            
            <div className="overflow-hidden">
                <table className="min-w-full">
                    <thead>
                        <tr className="border-b border-slate-50 text-slate-400 text-[10px] uppercase tracking-wider font-semibold">
                            <th className="pb-3 text-left pl-2">容器名称</th>
                            <th className="pb-3 text-left">状态</th>
                            <th className="pb-3 text-right pr-2">控制</th>
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-50">
                        {isLoading ? (
                            <tr><td colSpan={3} className="py-8 text-center text-slate-400">加载中...</td></tr>
                        ) : containers.length === 0 ? (
                            <tr>
                                <td colSpan={3} className="py-12 text-center text-slate-400">
                                    <div className="flex flex-col items-center gap-2">
                                        <AlertTriangle size={24} className="text-slate-200" />
                                        <span>未找到容器或 Docker 服务不可达</span>
                                    </div>
                                </td>
                            </tr>
                        ) : containers.map(c => (
                            <tr key={c.id} className="group hover:bg-slate-50/50 transition-colors">
                                <td className="py-4 pl-2">
                                    <div className="flex items-center gap-3">
                                        <div className="w-8 h-8 rounded-lg bg-slate-100 flex items-center justify-center text-slate-400">
                                            <Box size={16} />
                                        </div>
                                        <div className="flex flex-col">
                                            <span className="text-sm font-semibold text-slate-700">{c.name}</span>
                                            <span className="text-[10px] text-slate-400 font-mono truncate max-w-[120px]">{c.image}</span>
                                        </div>
                                    </div>
                                </td>
                                <td className="py-4">
                                    <span className={`inline-flex items-center px-2 py-1 rounded-md text-[10px] font-bold uppercase tracking-tight ${
                                        c.status === 'running' 
                                        ? 'bg-green-50 text-green-600' 
                                        : 'bg-red-50 text-red-600'
                                    }`}>
                                        <span className={`w-1.5 h-1.5 rounded-full mr-1.5 ${c.status === 'running' ? 'bg-green-500' : 'bg-red-500'}`}></span>
                                        {c.status}
                                    </span>
                                </td>
                                <td className="py-4 pr-2">
                                    <div className="flex justify-end gap-1.5">
                                        {c.status === 'running' ? (
                                            <button 
                                                onClick={() => handleAction(c.id, 'stop')} 
                                                className="p-1.5 hover:bg-red-100 text-red-400 rounded-lg transition-colors tooltip"
                                                title="停止"
                                            >
                                                <Square size={14} fill="currentColor" />
                                            </button>
                                        ) : (
                                            <button 
                                                onClick={() => handleAction(c.id, 'start')} 
                                                className="p-1.5 hover:bg-green-100 text-green-400 rounded-lg transition-colors"
                                                title="开启"
                                            >
                                                <Play size={14} fill="currentColor" />
                                            </button>
                                        )}
                                        <button 
                                            onClick={() => handleAction(c.id, 'restart')} 
                                            className="p-1.5 hover:bg-blue-100 text-blue-400 rounded-lg transition-colors"
                                            title="重启"
                                        >
                                            <RotateCcw size={14} />
                                        </button>
                                    </div>
                                </td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </div>
        </div>
    );
};
