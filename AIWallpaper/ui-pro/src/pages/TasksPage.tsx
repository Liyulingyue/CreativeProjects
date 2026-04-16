import React from 'react';

interface TasksPageProps {
  autoRefreshHours: number;
  setAutoRefreshHours: (v: number) => void;
  autoPrompt: string;
  setAutoPrompt: (v: string) => void;
  handleSaveKey: () => void;
}

const TasksPage: React.FC<TasksPageProps> = ({
  autoRefreshHours, setAutoRefreshHours,
  autoPrompt, setAutoPrompt,
  handleSaveKey
}) => {
  return (
    <div className="space-y-10 animate-in fade-in slide-in-from-bottom-6 duration-700">
      <div className="bg-white border border-slate-200 rounded-[2.5rem] p-10 shadow-xl relative overflow-hidden group">
        <div className="absolute top-0 right-0 p-8 opacity-5 pointer-events-none group-hover:scale-110 transition-transform duration-1000">
          <svg xmlns="http://www.w3.org/2000/svg" width="120" height="120" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" strokeLinecap="round" strokeLinejoin="round" className="text-blue-500"><path d="M21 12a9 9 0 1 1-6.219-8.56" /><path d="M22 10 16 12l3 5 3-7Z" /></svg>
        </div>
        <h2 className="text-3xl font-bold mb-8 flex items-center gap-3 text-slate-900"><span className="w-1.5 h-8 bg-blue-600 rounded-full"></span>高级自动化</h2>
        <p className="text-slate-500 mb-10 max-w-xl font-medium leading-relaxed text-lg">
          开启后系统将定期为您生成并更换新壁纸。让灵感在屏幕上跳动，无需亲自动手。
        </p>

        <div className="space-y-10">
          <div className="flex items-center justify-between p-8 bg-slate-50 rounded-[2rem] border border-slate-100">
            <div className="flex items-center gap-6">
              <div className={`p-4 rounded-2xl shadow-lg transition-all duration-500 ${autoRefreshHours > 0 ? "bg-blue-600 text-white" : "bg-white text-slate-400 border border-slate-200"}`}>
                <svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" /></svg>
              </div>
              <div>
                <h3 className="text-xl font-black text-slate-900">自动刷新壁纸</h3>
                <p className="text-sm text-slate-400 font-bold uppercase tracking-wider mt-1">{autoRefreshHours > 0 ? "已开启" : "已禁用"}</p>
              </div>
            </div>
            <button
              onClick={() => {
                if (autoRefreshHours > 0) setAutoRefreshHours(0);
                else setAutoRefreshHours(4);
              }}
              className={`w-16 h-9 rounded-full transition-all duration-500 relative ${autoRefreshHours > 0 ? 'bg-blue-600 shadow-xl shadow-blue-200' : 'bg-slate-200'}`}
            >
              <div className={`absolute top-1.5 w-6 h-6 bg-white rounded-full transition-all duration-500 shadow-sm ${autoRefreshHours > 0 ? 'left-8' : 'left-1.5'}`}></div>
            </button>
          </div>

          {autoRefreshHours > 0 && (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-10 animate-in fade-in slide-in-from-top-4 duration-500">
              <div className="space-y-6 bg-white border border-slate-100 p-8 rounded-[2rem] shadow-sm">
                <label className="text-xs font-black text-slate-400 uppercase flex justify-between tracking-[0.2em] mb-4">
                  <span>刷新间隔时间</span>
                  <span className="text-blue-600 font-black">{autoRefreshHours} 小时</span>
                </label>
                <input
                  type="range" min="1" max="72" step="1"
                  value={autoRefreshHours}
                  onChange={(e) => setAutoRefreshHours(parseInt(e.target.value))}
                  className="w-full h-2 bg-slate-100 rounded-lg appearance-none cursor-pointer accent-blue-600"
                />
                <div className="flex justify-between text-[10px] text-slate-300 font-black">
                  <span>1小时</span>
                  <span>72小时</span>
                </div>
              </div>

              <div className="space-y-6 bg-white border border-slate-100 p-8 rounded-[2rem] shadow-sm">
                <label className="text-xs font-black text-slate-400 uppercase flex justify-between tracking-[0.2em] mb-4">
                  <span>自动化提示词</span>
                  <span className="text-slate-300">留空则随机</span>
                </label>
                <textarea
                  value={autoPrompt}
                  onChange={(e) => setAutoPrompt(e.target.value)}
                  placeholder="设定专属的自动化主题..."
                  className="w-full bg-slate-50 border border-slate-200 rounded-xl p-4 text-sm focus:ring-4 focus:ring-blue-500/10 focus:border-blue-500/50 outline-none transition-all resize-none shadow-sm placeholder:text-slate-300 text-slate-600 min-h-[80px]"
                />
              </div>
            </div>
          )}

          <div className="pt-6">
            <button
              onClick={handleSaveKey}
              className="bg-slate-900 hover:bg-black text-white px-12 py-4 rounded-[1.25rem] font-black text-xl shadow-2xl shadow-slate-200 active:scale-95 transition-all flex items-center gap-3 w-full justify-center"
            >
              保存自动化配置
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" /><polyline points="17 21 17 13 7 13 7 21" /><polyline points="7 3 7 8 15 8" /></svg>
            </button>
          </div>
        </div>
      </div>

      <p className="text-center text-[11px] text-slate-300 font-bold uppercase tracking-widest pb-10">
        提示：自动化任务将在后台运行，即使关闭主界面（不退出程序）也会生效
      </p>
    </div>
  );
};

export default TasksPage;
