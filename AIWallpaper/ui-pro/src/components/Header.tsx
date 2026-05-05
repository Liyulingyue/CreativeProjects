import React from 'react';

interface HeaderProps {
    activeTab: string;
    setActiveTab: (tab: string) => void;
    sendIpc: (cmd: string, arg?: any) => void;
    uiMode: 'lite' | 'pro';
    setUiMode: (mode: 'lite' | 'pro') => void;
}

const Header: React.FC<HeaderProps> = ({ activeTab, setActiveTab, sendIpc, uiMode, setUiMode }) => {
    const toggleMode = () => {
        const newMode = uiMode === 'lite' ? 'pro' : 'lite';
        setUiMode(newMode);
        sendIpc("switch_mode", newMode);
    };

    return (
        <header className={`flex items-center justify-between px-6 bg-white/70 backdrop-blur-xl border-b border-slate-200 sticky top-0 z-50 transition-all duration-300 py-4`}>
            <div className="flex items-center gap-3 group cursor-pointer" onClick={toggleMode}>
                <div className={`p-2 rounded-xl shadow-lg transition-transform duration-300 group-hover:scale-110 bg-blue-600 shadow-blue-500/20`}>
                    <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1-1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z" /></svg>
                </div>
                <span className="font-bold text-xl tracking-tight text-slate-900">
                    AIWallpaper 
                    <span className={`text-[10px] font-bold ml-1 px-1.5 py-0.5 rounded border uppercase align-middle transition-colors text-blue-600 bg-blue-50 border-blue-100`}>
                        {uiMode.toUpperCase()}
                    </span>
                </span>
            </div>
            
            {uiMode === 'pro' && (
                <nav className="flex items-center gap-1 bg-slate-100 p-1 rounded-2xl border border-slate-200 shadow-inner">
                    {[
                        { id: "create", label: "创作" }, 
                        { id: "gallery", label: "画廊" }, 
                        { id: "edit", label: "编辑" },
                        { id: "tasks", label: "自动化" }, 
                        { id: "settings", label: "设置" }
                    ].map(tab => (
                        <button key={tab.id} onClick={() => setActiveTab(tab.id)} className={`px-5 py-2 rounded-xl text-sm font-semibold transition-all duration-300 ${activeTab === tab.id ? "bg-white text-blue-600 shadow-sm border border-slate-200" : "text-slate-500 hover:text-slate-800 hover:bg-white/50"}`}>{tab.label}</button>
                    ))}
                </nav>
            )}

            {uiMode === 'lite' && (
                <nav className="flex items-center gap-1 bg-slate-100 p-1 rounded-2xl border border-slate-200 shadow-inner">
                    <button onClick={() => setActiveTab('create')} className={`px-4 py-2 rounded-xl text-sm transition-all duration-300 ${activeTab === 'create' ? 'bg-white text-blue-600 shadow-sm border border-slate-200' : 'text-slate-400 hover:text-slate-600 hover:bg-white/50'}`} title="创作">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z"/></svg>
                    </button>
                    <button onClick={() => setActiveTab('settings')} className={`px-4 py-2 rounded-xl text-sm transition-all duration-300 ${activeTab === 'settings' ? 'bg-white text-blue-600 shadow-sm border border-slate-200' : 'text-slate-400 hover:text-slate-600 hover:bg-white/50'}`} title="配置">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></svg>
                    </button>
                </nav>
            )}
            <div className="flex items-center gap-2">
                <button 
                    onClick={() => sendIpc("minimize")} 
                    className="p-2.5 hover:bg-slate-100 rounded-xl text-slate-400 hover:text-slate-600 transition-all active:scale-90 group relative"
                    title="最小化到托盘"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="M5 12h14" /></svg>
                    <span className="absolute -bottom-10 left-1/2 -translate-x-1/2 bg-slate-800 text-white text-[10px] px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap z-[100]">最小化到托盘</span>
                </button>
                <button 
                    onClick={() => sendIpc("close")} 
                    className="p-2.5 hover:bg-red-50 hover:text-red-500 rounded-xl text-slate-400 transition-all active:scale-90 group relative"
                    title="关闭"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="M18 6 6 18" /><path d="m6 6 12 12" /></svg>
                    <span className="absolute -bottom-10 left-1/2 -translate-x-1/2 bg-slate-800 text-white text-[10px] px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap z-[100]">关闭</span>
                </button>
            </div>
        </header>
    );
};

export default Header;
