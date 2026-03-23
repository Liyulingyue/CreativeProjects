import React, { useEffect, useState } from 'react';
import { api } from '../api';
import type { SystemInfo } from '../types.ts';
import { Cpu, Database, HardDrive, Activity } from 'lucide-react';

export const SystemMonitor: React.FC = () => {
    const [info, setInfo] = useState<SystemInfo | null>(null);

    useEffect(() => {
        const fetchInfo = async () => {
            try {
                const data = await api.getSystemInfo();
                setInfo(data);
            } catch (error) {
                console.error('Failed to fetch system info:', error);
            }
        };
        fetchInfo();
        const timer = setInterval(fetchInfo, 3000);
        return () => clearInterval(timer);
    }, []);

    if (!info) return (
        <div className="flex items-center justify-center p-12">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-indigo-600"></div>
        </div>
    );

    const gb = (bytes: number) => (bytes / (1024 ** 3)).toFixed(1);

    const StatCard = ({ title, value, detail, percent, icon: Icon, colorClass }: any) => (
        <div className="bg-white p-6 rounded-2xl shadow-sm border border-slate-100 hover:shadow-md transition-shadow">
            <div className="flex justify-between items-start mb-4">
                <div className="flex flex-col">
                  <h3 className="text-slate-500 text-sm font-medium mb-1">{title}</h3>
                  <span className="text-2xl font-bold text-slate-800">{value}</span>
                </div>
                <div className={`p-2.5 rounded-xl ${colorClass.bg}`}>
                    <Icon size={20} className={colorClass.text} />
                </div>
            </div>
            <div className="w-full bg-slate-100 h-2 rounded-full overflow-hidden mb-3">
                <div 
                    className={`h-full transition-all duration-700 ease-out ${colorClass.bar}`}
                    style={{ width: `${percent}%` }}
                />
            </div>
            <div className="flex justify-between items-center text-[10px] text-slate-400 font-medium tracking-tight">
               <span>详情</span>
               <span className="font-mono uppercase">{detail}</span>
            </div>
        </div>
    );

    return (
        <div className="space-y-4">
            <div className="flex items-center gap-2 px-1">
                <Activity size={18} className="text-indigo-600" />
                <h2 className="text-base font-bold text-slate-700 uppercase tracking-wider">实时资源监控</h2>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
                <StatCard 
                    title="CPU 使用率"
                    value={`${info.cpu_percent}%`}
                    detail={`${info.os} ${info.os_release}`}
                    percent={info.cpu_percent}
                    icon={Cpu}
                    colorClass={{ bg: 'bg-blue-50', text: 'text-blue-600', bar: 'bg-blue-600' }}
                />
                <StatCard 
                    title="内存使用"
                    value={`${info.memory.percent}%`}
                    detail={`${gb(info.memory.used)}G / ${gb(info.memory.total)}G`}
                    percent={info.memory.percent}
                    icon={Database}
                    colorClass={{ bg: 'bg-emerald-50', text: 'text-emerald-600', bar: 'bg-emerald-600' }}
                />
                <StatCard 
                    title="磁盘空间"
                    value={`${info.disk.percent}%`}
                    detail={`${gb(info.disk.used)}G / ${gb(info.disk.total)}G`}
                    percent={info.disk.percent}
                    icon={HardDrive}
                    colorClass={{ bg: 'bg-amber-50', text: 'text-amber-600', bar: 'bg-amber-600' }}
                />
            </div>
        </div>
    );
};
