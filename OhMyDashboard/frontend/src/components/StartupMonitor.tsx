import React, { useEffect, useState } from 'react';
import { api } from '../api';
import type { StartupItem } from '../types.ts';
import { Power, Info, ToggleLeft, ToggleRight, ListChecks } from 'lucide-react';

export const StartupMonitor: React.FC = () => {
    const [items, setItems] = useState<StartupItem[]>([]);
    const [isLoading, setIsLoading] = useState(true);

    useEffect(() => {
        const fetchStartup = async () => {
            const data = await api.getStartupInfo();
            setItems(data.startup_items || []);
            setIsLoading(false);
        };
        fetchStartup();
    }, []);

    return (
        <div className="bg-white p-6 rounded-2xl shadow-sm border border-slate-100 flex-1">
            <div className="flex items-center justify-between mb-6">
                <div className="flex items-center gap-2">
                    <ListChecks size={20} className="text-emerald-600" />
                    <h2 className="text-lg font-bold text-slate-800">系统自启项</h2>
                </div>
                <div className="flex items-center gap-1.5 text-[10px] text-slate-400 font-bold uppercase tracking-tight">
                    <Info size={12} className="text-slate-300" />
                    <span>仅限 Systemd 服务</span>
                </div>
            </div>

            <div className="overflow-y-auto max-h-[360px] scrollbar-hide">
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    {isLoading ? (
                        <div className="col-span-full py-8 text-center text-slate-400">加载中...</div>
                    ) : items.length === 0 ? (
                        <div className="col-span-full py-12 text-center text-slate-400 italic">未发现自启动服务</div>
                    ) : items.map((item, idx) => (
                        <div key={idx} className="flex items-center justify-between p-3 rounded-xl border border-slate-50 hover:bg-slate-50 transition-all group">
                            <div className="flex items-center gap-3 overflow-hidden">
                                <div className="p-2 rounded-lg bg-emerald-50 text-emerald-500 flex-shrink-0 group-hover:bg-emerald-100/50 transition-colors">
                                    <Power size={14} />
                                </div>
                                <span className="text-xs font-semibold text-slate-700 truncate font-mono" title={item.name}>
                                    {item.name.replace('.service', '')}
                                </span>
                            </div>
                            <div className="ml-2 flex items-center">
                                {item.status === 'enabled' ? (
                                    <ToggleRight className="text-emerald-500" size={18} fill="currentColor" opacity={0.6} />
                                ) : (
                                    <ToggleLeft className="text-slate-300" size={18} />
                                )}
                            </div>
                        </div>
                    ))}
                </div>
            </div>
        </div>
    );
};
